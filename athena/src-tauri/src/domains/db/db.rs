use chrono::Utc;
use base64::Engine;
use rusqlite::backup::Backup;
use rusqlite::ffi::sqlite3_auto_extension;
use rusqlite::OptionalExtension;
use rusqlite::{params, Connection};
use rusqlite::Transaction;
use serde::{Deserialize, Serialize};
use sqlite_vec::sqlite3_vec_init;
use std::fs;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::chats::{ChatSummary, SavedChat, SavedMessage};
use crate::config::load_config;
use crate::embeddings;

/// Register sqlite-vec once per process before opening any SQLite connection (must match bundled rusqlite).
fn ensure_sqlite_vec_auto_extension() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| unsafe {
        #[allow(clippy::missing_transmute_annotations)]
        let _ = sqlite3_auto_extension(Some(std::mem::transmute(sqlite3_vec_init as *const ())));
    });
}

pub fn is_stopword(w: &str) -> bool {
    matches!(
        w,
        "a" | "an"
            | "and"
            | "are"
            | "as"
            | "at"
            | "be"
            | "but"
            | "by"
            | "can"
            | "could"
            | "did"
            | "do"
            | "does"
            | "for"
            | "from"
            | "had"
            | "has"
            | "have"
            | "how"
            | "i"
            | "if"
            | "in"
            | "into"
            | "is"
            | "it"
            | "its"
            | "let"
            | "me"
            | "my"
            | "of"
            | "on"
            | "or"
            | "our"
            | "so"
            | "that"
            | "the"
            | "their"
            | "them"
            | "then"
            | "there"
            | "these"
            | "they"
            | "this"
            | "to"
            | "was"
            | "we"
            | "were"
            | "what"
            | "when"
            | "where"
            | "which"
            | "who"
            | "why"
            | "will"
            | "with"
            | "you"
            | "your"
    )
}

/// Deletes `.p2p-part` temp files older than `hours` under `Unsorted/`.
/// Runs in a blocking context (startup pruning job).
pub fn prune_orphan_p2p_parts_in_unsorted(app_data_dir: &Path, hours: u64) {
    let cutoff_secs = hours.max(1).saturating_mul(3600);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_secs();
    let unsorted = crate::library_ingest::unsorted_dir(app_data_dir);
    if !unsorted.is_dir() {
        return;
    }
    let entries = match fs::read_dir(&unsorted) {
        Ok(v) => v,
        Err(_) => return,
    };
    for ent in entries.flatten() {
        let p = ent.path();
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !name.ends_with(".p2p-part") {
            continue;
        }
        let meta = match ent.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        let modified_secs = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(now);
        if now.saturating_sub(modified_secs) >= cutoff_secs {
            let _ = fs::remove_file(&p);
        }
    }
}

pub fn extract_keywords(query: &str, max: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in query
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_ascii_alphanumeric()))
    {
        let w = raw.to_lowercase();
        if w.len() < 3 {
            continue;
        }
        if is_stopword(&w) {
            continue;
        }
        if out.iter().any(|x| x == &w) {
            continue;
        }
        out.push(w);
        if out.len() >= max {
            break;
        }
    }
    out
}


fn fts_keywords_to_match_query(keywords: &[String]) -> Option<String> {
    let parts: Vec<String> = keywords
        .iter()
        .map(|k| k.trim().to_lowercase())
        .filter(|k| !k.is_empty())
        .map(|k| {
            let e = k.replace('"', "\"\"");
            format!("\"{e}\"")
        })
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" OR "))
    }
}


/// Default lock-screen login when creating the architect row from scratch (case-insensitive).
/// The `athena_architect` badge is assigned by migration to whoever holds the bootstrapped architect
/// PIN, or to this login name if no PIN match exists; renaming the login does not revoke the badge.
pub const PERSONNEL_ARCHITECT_LOGIN: &str = "demo-operator";

/// SHA-256 hex digest of trimmed UTF-8 PIN bytes (interim; prefer Argon2 for production hardening).
pub fn personnel_pin_hash(pin: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(pin.trim().as_bytes());
    format!("{:x}", h.finalize())
}

#[derive(Debug, Clone)]
pub struct PersonnelRow {
    pub id: String,
    pub name: String,
    pub division: String,
    pub rank_level: i32,
    pub clearance_level: i32,
    pub pin_hash: String,
    pub display_name: Option<String>,
    pub operational_status: String,
    /// e.g. Infrastructure, Vanguard, Sustenance, InformationMedical
    pub directorate: Option<String>,
    pub tier: Option<String>,
    pub job_title: Option<String>,
    /// 1 = designated ATHENA creator (designer) seat; never implied by `rank_level` alone.
    pub athena_architect: i32,
    /// Hierarchy domain, e.g. "Engineering and Manufacturing"
    pub domain: Option<String>,
    /// Level 1 role (specific role)
    pub role_l1: Option<String>,
    /// Level 2 role (sector leader)
    pub role_l2: Option<String>,
    /// Level 3 role (directorate)
    pub role_l3: Option<String>,
    /// Level 4 role (executive)
    pub role_l4: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonnelPublic {
    pub id: String,
    pub name: String,
    pub division: String,
    pub rank_level: i32,
    pub clearance_level: i32,
    pub created_at: i64,
    pub display_name: Option<String>,
    pub operational_status: String,
    pub directorate: Option<String>,
    pub tier: Option<String>,
    pub job_title: Option<String>,
    #[serde(default)]
    pub athena_architect: bool,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub role_l1: Option<String>,
    #[serde(default)]
    pub role_l2: Option<String>,
    #[serde(default)]
    pub role_l3: Option<String>,
    #[serde(default)]
    pub role_l4: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InventoryItemRow {
    pub id: String,
    pub division: String,
    pub item_name: String,
    pub quantity: i64,
    pub last_updated: i64,
    pub category: String,
    pub directorate: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InventoryLogRow {
    pub id: String,
    pub item_id: String,
    pub user_id: String,
    pub action_type: String,
    pub old_qty: Option<i64>,
    pub new_qty: Option<i64>,
    pub timestamp: i64,
    pub caesar_fingerprint: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LogisticsLibraryRow {
    pub entry_id: String,
    pub body: String,
    pub created_at: i64,
    pub author_name: String,
    pub author_rank_level: Option<i32>,
    pub author_division: Option<String>,
    pub author_directorate: Option<String>,
    pub author_tier: Option<String>,
    pub author_job_title: Option<String>,
    pub section: String,
    pub proposal_id: Option<String>,
    pub proposal_status: Option<String>,
    pub map_lat: Option<f64>,
    pub map_lon: Option<f64>,
    pub saved_place_id: Option<String>,
    pub location_text: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InventoryProposalItemRow {
    pub id: String,
    pub action: String,
    pub division: String,
    pub item_name: String,
    pub category: String,
    pub quantity: i64,
    pub confidence: Option<f64>,
    /// Free-text or normalized location for this line (dedupe / display).
    pub line_location: Option<String>,
}

/// One indexed library row for mesh P2P manifest exchange.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshManifestRow {
    pub path: String,
    pub file_type: String,
    pub size_bytes: u64,
    /// Unix time (seconds) from `indexed_files.modified_at`, for P2P conflict resolution.
    #[serde(default)]
    pub modified_at: Option<i64>,
}

pub struct Database {
    conn: Mutex<Connection>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IndexedFile {
    pub id: i64,
    pub path: String,
    pub file_type: String,
    pub name: String,
    pub size: Option<i64>,
    pub modified_at: Option<i64>,
    pub text_content: Option<String>,
    pub metadata_json: Option<String>,
    pub created_at: i64,
    pub display_title: Option<String>,
    pub page_count: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArchiveWatchRootRow {
    pub id: i64,
    pub path: String,
    pub label: Option<String>,
    pub sort_index: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChunkEmbedding {
    pub id: i64,
    pub file_path: String,
    pub chunk_index: i64,
    pub chunk_text: String,
    pub vector: Vec<f32>,
    /// Seconds from start of source media (e.g. VTT cue), for temporal RAG citations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp_start: Option<f64>,
}

/// One expanded context block for hybrid retrieval + reranking (Phase 3).
#[derive(Debug, Clone, Serialize)]
pub struct HybridSearchHit {
    /// Child chunk text (the passage that matched).
    pub expanded_text: String,
    pub display_title: Option<String>,
    pub page_number: Option<i64>,
    pub file_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp_start: Option<f64>,
    /// Cross-encoder relevance logit when the reranker model is loaded, fused rank score otherwise.
    pub score: f32,
    /// Parent section this chunk belongs to, when the document was indexed with parent/child chunks.
    #[serde(skip)]
    pub parent_id: Option<i64>,
    /// Text of the parent section, for context expansion when the model's window has room.
    #[serde(skip)]
    pub parent_text: Option<String>,
}

/// A reranked candidate plus the retrieval pipeline's verdict on it.
#[derive(Debug, Clone, Serialize)]
pub struct ScoredHybridHit {
    #[serde(flatten)]
    pub hit: HybridSearchHit,
    /// True when the chunk cleared the relevance cutoff and would be offered to the model.
    pub relevant: bool,
    /// True when `score` came from the cross-encoder rather than fused rank.
    pub reranked: bool,
}

/// Upper bound on chunks `hybrid_search` returns. How many reach the model is decided by the
/// context budget in `assemble_library_context`.
pub const HYBRID_SEARCH_TOP_CHUNKS: usize = 12;
/// Without a cross-encoder there is no relevance score to cut on, so keep fewer.
pub const HYBRID_SEARCH_TOP_CHUNKS_NO_RERANKER: usize = 6;

/// Cross-encoder logit below which a chunk is treated as not answering the question at all.
pub const HYBRID_RERANK_MIN_LOGIT: f32 = -5.0;
/// A chunk must also be within this many logits of the best chunk for the query.
pub const HYBRID_RERANK_MAX_GAP: f32 = 6.0;

/// Relevance cutoff for cross-encoder scores: an absolute floor plus a gap to the best hit, so a
/// strong match suppresses weak ones while a query with only moderate matches still returns them.
pub fn hybrid_rerank_score_is_relevant(score: f32, top_score: f32) -> bool {
    score >= HYBRID_RERANK_MIN_LOGIT && score >= top_score - HYBRID_RERANK_MAX_GAP
}

/// Row bundle for hybrid RAG diversification.
#[derive(Debug, Clone)]
struct HybridChunkRow {
    retrieval_text: String,
    chunk_text: String,
    parent_id: Option<i64>,
    file_path: String,
    page_start: Option<i64>,
    display_title: Option<String>,
    timestamp_start: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct IndexStats {
    pub total_files: i64,
    pub text_count: i64,
    pub image_count: i64,
    pub video_count: i64,
    pub map_count: i64,
    pub zim_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RagCounts {
    pub document_count: i64,
    pub chunk_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TacticalPointOfInterest {
    pub id: String,
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub notes: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TacticalMeshNodeLocation {
    pub node_id: String,
    pub label: String,
    pub lat: f64,
    pub lon: f64,
    pub last_seen_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapMarker {
    pub id: String,
    pub lat: f64,
    pub lng: f64,
    pub title: String,
    pub description: String,
    pub created_at: String,
    /// Tactical category: `hazard`, `water`, `cache`, `med`, etc.
    #[serde(default = "default_map_marker_category")]
    pub category: String,
}

fn default_map_marker_category() -> String {
    "hazard".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapDrawing {
    pub id: String,
    pub feature_type: String,
    pub geometry_json: String,
    pub style_json: String,
    pub created_at: String,
}

/// User-saved map location (personal or shared over LAN when `scope == "shared"`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedPlaceRow {
    pub id: String,
    pub name: String,
    pub lat: f64,
    pub lng: f64,
    #[serde(default = "default_saved_place_icon")]
    pub icon: String,
    #[serde(default = "default_saved_place_color")]
    pub color: String,
    /// `"personal"` or `"shared"`.
    pub scope: String,
    pub owner_personnel_id: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub deleted_at: Option<String>,
}

fn default_saved_place_icon() -> String {
    "pin".to_string()
}

fn default_saved_place_color() -> String {
    "#3388ff".to_string()
}

/// Proposed or active routing obstruction (GeoJSON geometry in `geometry_json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteBlockageRow {
    pub id: String,
    pub geometry_json: String,
    #[serde(default)]
    pub reason: String,
    /// `impassable` or `slow`.
    pub severity: String,
    /// Comma-separated: walk,drive,bike,offroad
    #[serde(default = "default_blockage_modes")]
    pub modes_affected: String,
    pub starts_at: String,
    #[serde(default)]
    pub expires_at: Option<String>,
    /// pending, active, rejected, expired
    pub status: String,
    pub proposed_by_personnel_id: String,
    #[serde(default)]
    pub approved_by_personnel_id: Option<String>,
    #[serde(default)]
    pub approved_at: Option<String>,
    #[serde(default)]
    pub notes: String,
    pub created_at: String,
    pub updated_at: String,
}

fn default_blockage_modes() -> String {
    "walk,drive,bike,offroad".to_string()
}

/// Tactical area-of-control polygon (GeoJSON `Polygon` in `geometry_json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerritoryRow {
    pub id: String,
    pub name: String,
    pub status: String,
    pub color: String,
    pub geometry_json: String,
    pub created_at: String,
    pub updated_at: String,
    pub created_by_personnel_id: String,
    #[serde(default)]
    pub deleted_at: Option<String>,
}

/// AI- or user-proposed change awaiting rank-4+ review (or auto-approved).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiStagedChangeRow {
    pub id: String,
    pub entity_type: String,
    pub action: String,
    #[serde(default)]
    pub target_entity_id: Option<String>,
    pub payload_json: String,
    #[serde(default)]
    pub diff_summary: String,
    pub source: String,
    #[serde(default)]
    pub proposed_by_personnel_id: Option<String>,
    pub status: String,
    #[serde(default)]
    pub reviewed_by_personnel_id: Option<String>,
    #[serde(default)]
    pub reviewed_at: Option<String>,
    #[serde(default)]
    pub review_notes: Option<String>,
    #[serde(default)]
    pub auto_approve_rule_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiAutoapproveRuleRow {
    pub id: String,
    pub entity_type: String,
    pub action: String,
    pub predicate_json: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub created_by_personnel_id: String,
    pub created_at: String,
}

fn default_true() -> bool {
    true
}

/// Built Valhalla routing tile set for a region.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingRegionRow {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub bbox_json: String,
    #[serde(default)]
    pub source_pbf_path: Option<String>,
    #[serde(default)]
    pub tiles_path: String,
    pub build_status: String,
    #[serde(default)]
    pub modes_built: String,
    #[serde(default)]
    pub size_bytes: i64,
    pub created_at: String,
    /// RFC3339 timestamp when offline geocode FTS was last built for this region.
    #[serde(default)]
    pub geocode_built_at: Option<String>,
}

/// One hit from the offline geocoder (FTS over OSM-derived features).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeocodeMatch {
    pub id: String,
    pub region_id: String,
    pub feature_class: String,
    pub label: String,
    pub lat: f64,
    pub lon: f64,
    pub importance: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RadioLog {
    pub id: String,
    pub direction: String,
    pub message: String,
    pub timestamp: String,
}

/// RSSI/SNR sample from the radio serial line `ATH:SIG rssi=<int> snr=<float>`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RadioTelemetrySample {
    pub id: i64,
    pub timestamp: String,
    pub source_node_id: u32,
    pub rssi: i64,
    pub snr: f64,
}

/// Outgoing mesh packet tracked until `ATH:ACK id=...` is received.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshMessage {
    pub packet_id: String,
    pub message: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Operator-defined Meshtastic node profile (Counter-Sign).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeshProfile {
    pub id: String,
    pub node_id: i64,
    pub username: String,
    pub counter_sign: String,
    pub created_at: i64,
    /// 0 = normal, 1 = active emergency, 2 = suspended pending verification.
    #[serde(default)]
    pub sos_state: i32,
    #[serde(default = "default_mesh_profile_registered")]
    pub is_registered: bool,
    #[serde(default = "default_mesh_profile_rank")]
    pub rank_level: i32,
}

fn default_mesh_profile_registered() -> bool {
    true
}

fn default_mesh_profile_rank() -> i32 {
    1
}

/// SOS state values for `mesh_profiles.sos_state`.
pub const SOS_STATE_NORMAL: i32 = 0;
pub const SOS_STATE_ACTIVE: i32 = 1;
pub const SOS_STATE_SUSPENDED: i32 = 2;


/// One row in the mesh network chat log (remote, operator, or AI).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeshChatRow {
    pub id: String,
    pub node_id: i64,
    pub sender_type: String,
    pub message_type: String,
    pub content: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeshNetworkNodeSummary {
    pub node_id: u32,
    pub username: Option<String>,
    pub last_message_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleCatalogEntry {
    pub id: String,
    pub domain: String,
    /// "l1" | "l2" | "l3" | "l4"
    pub level: String,
    pub title: String,
    pub created_at: i64,
}

/// L4/L5 command notifications (inventory, map, comms, personnel).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandNotificationRow {
    pub id: String,
    pub kind: String,
    pub severity: String,
    pub title: String,
    pub body: Option<String>,
    pub ref_id: Option<String>,
    pub created_at: i64,
    pub ack_by_user_id: Option<String>,
    pub ack_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryIntakeEventRow {
    pub id: String,
    pub logbook_entry_id: String,
    pub inventory_id: Option<String>,
    pub directorate: String,
    pub item_norm: String,
    pub location_norm: Option<String>,
    pub delta_qty: i64,
    pub submitter_user_id: Option<String>,
    pub reason: Option<String>,
    pub other_user_id: Option<String>,
    pub created_at: i64,
}

/// Stored encrypted channel profile (frequency + PSK); never written to `radio_logs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RadioChannel {
    pub id: String,
    pub name: String,
    pub frequency_mhz: f64,
    pub encryption_key: String,
    pub created_at: String,
}

fn default_logbook_notebook() -> String {
    "general".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogbookEntry {
    pub id: String,
    pub body: String,
    pub created_at: i64,
    /// Set when the entry was received from another node over Meshtastic (`None` = created on this device).
    pub from_node_id: Option<u32>,
    #[serde(default)]
    pub author_name: String,
    #[serde(default)]
    pub author_occupation: String,
    /// Authenticated submitter (NULL for mesh/legacy).
    #[serde(default)]
    pub user_id: Option<String>,
    /// Authenticated submitter rank (NULL for mesh/legacy).
    #[serde(default)]
    pub author_rank_level: Option<i32>,
    /// Authenticated submitter division (NULL for mesh/legacy).
    #[serde(default)]
    pub author_division: Option<String>,
    #[serde(default)]
    pub author_directorate: Option<String>,
    #[serde(default)]
    pub author_tier: Option<String>,
    #[serde(default)]
    pub author_job_title: Option<String>,
    #[serde(default)]
    pub map_lat: Option<f64>,
    #[serde(default)]
    pub map_lon: Option<f64>,
    #[serde(default)]
    pub saved_place_id: Option<String>,
    #[serde(default)]
    pub location_text: Option<String>,
    #[serde(default)]
    pub parent_entry_id: Option<String>,
    /// `general` = field logbook; `tactical` = restricted military / tactical notes (never sent over mesh).
    #[serde(default = "default_logbook_notebook")]
    pub notebook: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarEvent {
    pub id: String,
    pub title: String,
    pub notes: Option<String>,
    pub start_ts: i64,
    pub end_ts: i64,
    pub all_day: bool,
    pub location_label: Option<String>,
    pub linked_marker_id: Option<String>,
    #[serde(default)]
    pub participant_names: Vec<String>,
    pub reminder_minutes_before: Option<i64>,
    pub recurrence_rule: Option<String>,
    pub category: Option<String>,
    pub source: String,
    pub updated_at: i64,
    pub origin_node_id: Option<u32>,
    #[serde(default)]
    pub deleted: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcademyProgressRow {
    pub lesson_id: String,
    pub completed_at: i64,
}

fn sqlite_error_is_busy(err: &rusqlite::Error) -> bool {
    matches!(
        err,
        rusqlite::Error::SqliteFailure(embedded, _) if embedded.code == rusqlite::ErrorCode::DatabaseBusy
    )
}

/// Runs `op` under a short-lived connection lock; retries on transient `SQLITE_BUSY` so concurrent
/// post-replication ingests or external tools touching the DB file do not drop writes after heavy indexing work.
fn with_sql_connection_busy_retries<T>(
    conn_mutex: &Mutex<Connection>,
    mut op: impl FnMut(&Connection) -> rusqlite::Result<T>,
) -> Result<T, String> {
    const MAX_ATTEMPTS: u32 = 60;
    const SLEEP_MS: u64 = 100;
    for attempt in 0..MAX_ATTEMPTS {
        let conn = conn_mutex.lock().map_err(|e| e.to_string())?;
        match op(&conn) {
            Ok(v) => return Ok(v),
            Err(e) if sqlite_error_is_busy(&e) && attempt + 1 < MAX_ATTEMPTS => {
                drop(conn);
                std::thread::sleep(Duration::from_millis(SLEEP_MS));
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    Err("SQLite is busy (exceeded retry window).".to_string())
}

impl Database {
    /// Minimum cosine similarity for a chunk to be a semantic *candidate*.
    ///
    /// MiniLM-class sentence embeddings score a question against a passage that answers it at
    /// roughly 0.3–0.6, so the previous 0.65 floor discarded most genuine matches. This floor only
    /// removes clear noise; relevance is decided afterwards by the cross-encoder in `hybrid_search`.
    pub const RAG_MIN_SIMILARITY_SCORE: f32 = 0.30;

    pub fn new(db_path: &Path) -> Result<Self, String> {
        ensure_sqlite_vec_auto_extension();
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
        conn.execute_batch(
            "
            PRAGMA journal_mode=WAL;
            PRAGMA busy_timeout=5000;
            ",
        )
        .map_err(|e| e.to_string())?;
        // Hardening: validate the database file on every start to catch corruption early.
        // If this fails, callers should enter Maintenance Mode instead of continuing normally.
        let integrity: String = conn
            .query_row("PRAGMA integrity_check;", [], |row| row.get(0))
            .map_err(|e| e.to_string())?;
        if integrity.trim().to_ascii_lowercase() != "ok" {
            return Err(format!("SQLite integrity_check failed: {integrity}"));
        }
        let db = Database {
            conn: Mutex::new(conn),
        };
        db.init_schema()?;
        db.migrate_rag_phase1_schema()?;
        db.migrate_chunk_embeddings_needs_embedding()?;
        db.migrate_chunk_embeddings_timestamp_start()?;
        db.migrate_rag_documents_source_package()?;
        db.migrate_indexed_files_relative_path()?;
        db.migrate_library_chunks_fts()?;
        db.migrate_chunk_embeddings_vec384()?;
        db.migrate_map_schema()?;
        db.migrate_radio_telemetry_schema()?;
        db.migrate_logbook_schema()?;
        db.migrate_personnel_inventory_schema()?;
        db.migrate_roman_rbac_column_updates()?;
        db.migrate_logistics_library_schema()?;
        db.migrate_mesh_sos_schema()?;
        db.migrate_map_overhaul_schema()?;
        db.migrate_territories_schema()?;
        db.migrate_directorate_intake_schema()?;
        db.migrate_personnel_clearance_level()?;
        db.migrate_personnel_hierarchy_v5()?;
        db.migrate_personnel_architect_badge()?;
        db.migrate_role_catalog()?;
        db.migrate_command_notifications()?;
        db.migrate_perf_indices_v1()?;
        db.migrate_geocode_v1()?;
        db.migrate_routing_region_geocode_meta()?;
        crate::field_ops::store::ensure_schema(&db)?;
        Ok(db)
    }

    /// Opens an in-memory database with the same schema as on-disk installs. Used when swapping the
    /// on-disk file during restore so the previous [`Connection`] is fully dropped.
/// Run `f` with the SQLite connection. For feature modules that own their own tables
    /// (see `field_ops::store`) so this file does not have to grow for every new feature.
    pub(crate) fn with_conn<T>(
        &self,
        f: impl FnOnce(&Connection) -> Result<T, String>,
    ) -> Result<T, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        f(&conn)
    }

        pub fn new_in_memory_placeholder() -> Result<Self, String> {
        ensure_sqlite_vec_auto_extension();
        let conn = Connection::open(":memory:").map_err(|e| e.to_string())?;
        conn.execute_batch(
            "
            PRAGMA journal_mode=WAL;
            PRAGMA busy_timeout=5000;
            ",
        )
        .map_err(|e| e.to_string())?;
        let db = Database {
            conn: Mutex::new(conn),
        };
        db.init_schema()?;
        db.migrate_rag_phase1_schema()?;
        db.migrate_chunk_embeddings_needs_embedding()?;
        db.migrate_chunk_embeddings_timestamp_start()?;
        db.migrate_rag_documents_source_package()?;
        db.migrate_indexed_files_relative_path()?;
        db.migrate_library_chunks_fts()?;
        db.migrate_chunk_embeddings_vec384()?;
        db.migrate_map_schema()?;
        db.migrate_radio_telemetry_schema()?;
        db.migrate_logbook_schema()?;
        db.migrate_personnel_inventory_schema()?;
        db.migrate_roman_rbac_column_updates()?;
        db.migrate_logistics_library_schema()?;
        db.migrate_mesh_sos_schema()?;
        db.migrate_map_overhaul_schema()?;
        db.migrate_territories_schema()?;
        db.migrate_directorate_intake_schema()?;
        db.migrate_personnel_clearance_level()?;
        db.migrate_personnel_hierarchy_v5()?;
        db.migrate_personnel_architect_badge()?;
        db.migrate_role_catalog()?;
        db.migrate_command_notifications()?;
        db.migrate_perf_indices_v1()?;
        db.migrate_geocode_v1()?;
        db.migrate_routing_region_geocode_meta()?;
        crate::field_ops::store::ensure_schema(&db)?;
        Ok(db)
    }

    /// Copies the live database to `dest` using SQLite's online backup API (WAL-safe).
    pub fn backup_to_path(&self, dest: &Path) -> Result<(), String> {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let src = self.conn.lock().map_err(|e| e.to_string())?;
        let mut dst = Connection::open(dest).map_err(|e| e.to_string())?;
        dst.execute_batch("PRAGMA busy_timeout=5000;")
            .map_err(|e| e.to_string())?;
        let backup = Backup::new(&src, &mut dst).map_err(|e| e.to_string())?;
        backup
            .run_to_completion(5, Duration::from_millis(100), None)
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn init_schema(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS indexed_files (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                path TEXT UNIQUE NOT NULL,
                file_type TEXT NOT NULL,
                name TEXT NOT NULL,
                size INTEGER,
                modified_at INTEGER,
                text_content TEXT,
                metadata_json TEXT,
                created_at INTEGER NOT NULL,
                display_title TEXT,
                page_count INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_indexed_files_path ON indexed_files(path);
            CREATE INDEX IF NOT EXISTS idx_indexed_files_file_type ON indexed_files(file_type);
            CREATE TABLE IF NOT EXISTS chunk_embeddings (
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
                needs_embedding INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL,
                UNIQUE(file_path, chunk_index)
            );
            CREATE INDEX IF NOT EXISTS idx_chunk_embeddings_file_path ON chunk_embeddings(file_path);
            CREATE TABLE IF NOT EXISTS points_of_interest (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                lat REAL NOT NULL,
                lon REAL NOT NULL,
                notes TEXT NOT NULL DEFAULT '',
                updated_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_points_of_interest_lat_lon ON points_of_interest(lat, lon);
            CREATE TABLE IF NOT EXISTS mesh_node_locations (
                node_id TEXT PRIMARY KEY,
                label TEXT NOT NULL DEFAULT '',
                lat REAL NOT NULL,
                lon REAL NOT NULL,
                last_seen_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_mesh_node_locations_lat_lon ON mesh_node_locations(lat, lon);
            CREATE TABLE IF NOT EXISTS logbook_entries (
                id TEXT PRIMARY KEY NOT NULL,
                body TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                from_node_id INTEGER,
                author_name TEXT NOT NULL DEFAULT '',
                author_occupation TEXT NOT NULL DEFAULT '',
                parent_entry_id TEXT REFERENCES logbook_entries(id),
                notebook TEXT NOT NULL DEFAULT 'general'
            );
            CREATE INDEX IF NOT EXISTS idx_logbook_created_at ON logbook_entries(created_at DESC);
            CREATE TABLE IF NOT EXISTS calendar_events (
                id TEXT PRIMARY KEY NOT NULL,
                title TEXT NOT NULL,
                notes TEXT,
                start_ts INTEGER NOT NULL,
                end_ts INTEGER NOT NULL,
                all_day INTEGER NOT NULL DEFAULT 0,
                location_label TEXT,
                linked_marker_id TEXT,
                participant_names_json TEXT NOT NULL DEFAULT '[]',
                reminder_minutes_before INTEGER,
                recurrence_rule TEXT,
                category TEXT,
                source TEXT NOT NULL DEFAULT 'user',
                updated_at INTEGER NOT NULL,
                origin_node_id INTEGER,
                deleted INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_calendar_events_start_ts ON calendar_events(start_ts ASC);
            CREATE INDEX IF NOT EXISTS idx_calendar_events_updated_at ON calendar_events(updated_at DESC);
            CREATE TABLE IF NOT EXISTS rag_documents (
                id TEXT PRIMARY KEY NOT NULL,
                filename TEXT NOT NULL,
                file_type TEXT NOT NULL,
                total_chunks INTEGER NOT NULL,
                ingested_at DATETIME NOT NULL,
                source_package TEXT NOT NULL DEFAULT 'Downloads'
            );
            CREATE INDEX IF NOT EXISTS idx_rag_documents_ingested_at ON rag_documents(ingested_at DESC);
            CREATE TABLE IF NOT EXISTS rag_chunks (
                id TEXT PRIMARY KEY NOT NULL,
                document_id TEXT NOT NULL,
                chunk_index INTEGER NOT NULL,
                content TEXT NOT NULL,
                embedding BLOB,
                FOREIGN KEY(document_id) REFERENCES rag_documents(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_rag_chunks_document_id ON rag_chunks(document_id);
            CREATE UNIQUE INDEX IF NOT EXISTS idx_rag_chunks_document_chunk ON rag_chunks(document_id, chunk_index);
            CREATE TABLE IF NOT EXISTS rag_chunk_metadata (
                chunk_id TEXT PRIMARY KEY NOT NULL,
                source TEXT,
                content_type TEXT,
                tags_json TEXT,
                article_title TEXT,
                section_title TEXT,
                FOREIGN KEY(chunk_id) REFERENCES rag_chunks(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_rag_chunk_metadata_source ON rag_chunk_metadata(source);
            CREATE INDEX IF NOT EXISTS idx_rag_chunk_metadata_content_type ON rag_chunk_metadata(content_type);
            CREATE TABLE IF NOT EXISTS map_markers (
                id TEXT PRIMARY KEY NOT NULL,
                lat REAL NOT NULL,
                lng REAL NOT NULL,
                title TEXT NOT NULL,
                description TEXT NOT NULL,
                created_at DATETIME NOT NULL,
                category TEXT NOT NULL DEFAULT 'hazard'
            );
            CREATE INDEX IF NOT EXISTS idx_map_markers_created_at ON map_markers(created_at DESC);
            CREATE TABLE IF NOT EXISTS map_drawings (
                id TEXT PRIMARY KEY NOT NULL,
                feature_type TEXT NOT NULL,
                geometry_json TEXT NOT NULL,
                style_json TEXT NOT NULL,
                created_at DATETIME NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_map_drawings_created_at ON map_drawings(created_at DESC);
            CREATE TABLE IF NOT EXISTS radio_logs (
                id TEXT PRIMARY KEY NOT NULL,
                direction TEXT NOT NULL,
                message TEXT NOT NULL,
                timestamp DATETIME NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_radio_logs_timestamp ON radio_logs(timestamp DESC);
            CREATE TABLE IF NOT EXISTS radio_telemetry (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp DATETIME NOT NULL,
                source_node_id INTEGER NOT NULL DEFAULT 0,
                rssi INTEGER NOT NULL,
                snr REAL NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_radio_telemetry_timestamp ON radio_telemetry(timestamp DESC);
            CREATE TABLE IF NOT EXISTS mesh_messages (
                packet_id TEXT PRIMARY KEY NOT NULL,
                message TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at DATETIME NOT NULL,
                updated_at DATETIME NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_mesh_messages_created_at ON mesh_messages(created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_mesh_messages_status ON mesh_messages(status);
            CREATE TABLE IF NOT EXISTS mesh_profiles (
                id TEXT PRIMARY KEY NOT NULL,
                node_id INTEGER NOT NULL UNIQUE,
                username TEXT NOT NULL,
                counter_sign TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                sos_state INTEGER NOT NULL DEFAULT 0,
                is_registered INTEGER NOT NULL DEFAULT 1,
                rank_level INTEGER NOT NULL DEFAULT 1
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_mesh_profiles_node_id ON mesh_profiles(node_id);
            CREATE TABLE IF NOT EXISTS mesh_chats (
                id TEXT PRIMARY KEY NOT NULL,
                node_id INTEGER NOT NULL,
                sender_type TEXT NOT NULL CHECK (sender_type IN ('human','ai','remote')),
                message_type TEXT NOT NULL CHECK (message_type IN ('auth','chat','prompt','response','operator','emergency','clearance')),
                content TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_mesh_chats_node_created ON mesh_chats(node_id, created_at ASC);
            CREATE TABLE IF NOT EXISTS lan_chat_messages (
                id TEXT PRIMARY KEY NOT NULL,
                sender_node_id INTEGER NOT NULL,
                sender_label TEXT NOT NULL,
                content TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_lan_chat_messages_created ON lan_chat_messages(created_at ASC);
            CREATE TABLE IF NOT EXISTS radio_channels (
                id TEXT PRIMARY KEY NOT NULL,
                name TEXT NOT NULL,
                frequency_mhz REAL NOT NULL,
                encryption_key TEXT NOT NULL,
                created_at DATETIME NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_radio_channels_name ON radio_channels(name);
            CREATE TABLE IF NOT EXISTS academy_progress (
                lesson_id TEXT PRIMARY KEY NOT NULL,
                completed_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_academy_progress_completed ON academy_progress(completed_at DESC);
            CREATE TABLE IF NOT EXISTS archive_watch_roots (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                path TEXT NOT NULL UNIQUE,
                label TEXT,
                sort_index INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_archive_watch_roots_sort ON archive_watch_roots(sort_index ASC, id ASC);
            CREATE TABLE IF NOT EXISTS local_chats (
                id TEXT PRIMARY KEY NOT NULL,
                user_id TEXT NOT NULL,
                title TEXT NOT NULL,
                description TEXT,
                messages_json TEXT NOT NULL,
                context_summary TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_local_chats_user_updated ON local_chats(user_id, updated_at DESC);
            ",
        )
        .map_err(|e| e.to_string())?;
        if let Err(e) = conn.execute(
            "CREATE VIRTUAL TABLE IF NOT EXISTS chunk_embeddings_vec USING vec0( embedding float[384] );",
            [],
        ) {
            eprintln!("[athena] sqlite-vec: could not create chunk_embeddings_vec: {e}");
        }
        drop(conn);
        if let Err(e) = self.sync_chunk_embeddings_vec_table() {
            eprintln!("[athena] sqlite-vec: could not sync chunk_embeddings_vec: {e}");
        }
        Ok(())
    }

    /// Rebuild sqlite-vec shadow table from `chunk_embeddings` (e.g. after migration or corruption).
    pub fn rebuild_chunk_embeddings_vec(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        if conn
            .execute("DELETE FROM chunk_embeddings_vec", [])
            .is_err()
        {
            return Ok(());
        }
        let mut stmt = conn
            .prepare("SELECT id, vector FROM chunk_embeddings WHERE IFNULL(chunk_layer, '') != 'parent'")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?))
            })
            .map_err(|e| e.to_string())?;
        let pairs: Vec<(i64, Vec<u8>)> = rows.filter_map(|r| r.ok()).collect();
        drop(stmt);
        for (id, blob) in pairs {
            let expected = embeddings::EMBEDDING_DIM * std::mem::size_of::<f32>();
            if blob.len() != expected {
                continue;
            }
            let _ = conn.execute(
                "INSERT INTO chunk_embeddings_vec(rowid, embedding) VALUES (?1, ?2)",
                params![id, blob],
            );
        }
        Ok(())
    }

    fn sync_chunk_embeddings_vec_table(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let chunk_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM chunk_embeddings", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        let vec_count: i64 =
            match conn.query_row("SELECT COUNT(*) FROM chunk_embeddings_vec", [], |r| {
                r.get(0)
            }) {
                Ok(c) => c,
                Err(_) => return Ok(()),
            };
        if chunk_count == vec_count {
            return Ok(());
        }
        drop(conn);
        self.rebuild_chunk_embeddings_vec()
    }

    fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool, String> {
        let pragma = match table {
            "indexed_files" => "PRAGMA table_info(indexed_files)",
            "chunk_embeddings" => "PRAGMA table_info(chunk_embeddings)",
            "rag_documents" => "PRAGMA table_info(rag_documents)",
            _ => {
                return Err(format!("unknown table for migration: {}", table));
            }
        };
        let mut stmt = conn.prepare(pragma).map_err(|e| e.to_string())?;
        let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
        while let Some(row) = rows.next().map_err(|e| e.to_string())? {
            let name: String = row.get::<_, String>(1).map_err(|e| e.to_string())?;
            if name == column {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Phase 1 RAG: indexed_files titles/page_count; chunk_embeddings parent/child + pages + retrieval_text.
    fn migrate_rag_phase1_schema(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let _ = conn.execute("PRAGMA foreign_keys = ON", []);
        if !Self::column_exists(&conn, "indexed_files", "display_title")? {
            let _ = conn.execute(
                "ALTER TABLE indexed_files ADD COLUMN display_title TEXT",
                [],
            );
        }
        if !Self::column_exists(&conn, "indexed_files", "page_count")? {
            let _ = conn.execute(
                "ALTER TABLE indexed_files ADD COLUMN page_count INTEGER",
                [],
            );
        }
        if !Self::column_exists(&conn, "chunk_embeddings", "chunk_layer")? {
            let _ = conn.execute(
                "ALTER TABLE chunk_embeddings ADD COLUMN chunk_layer TEXT",
                [],
            );
        }
        if !Self::column_exists(&conn, "chunk_embeddings", "parent_id")? {
            let _ = conn.execute(
                "ALTER TABLE chunk_embeddings ADD COLUMN parent_id INTEGER REFERENCES chunk_embeddings(id) ON DELETE CASCADE",
                [],
            );
        }
        if !Self::column_exists(&conn, "chunk_embeddings", "page_start")? {
            let _ = conn.execute(
                "ALTER TABLE chunk_embeddings ADD COLUMN page_start INTEGER",
                [],
            );
        }
        if !Self::column_exists(&conn, "chunk_embeddings", "page_end")? {
            let _ = conn.execute(
                "ALTER TABLE chunk_embeddings ADD COLUMN page_end INTEGER",
                [],
            );
        }
        if !Self::column_exists(&conn, "chunk_embeddings", "retrieval_text")? {
            let _ = conn.execute(
                "ALTER TABLE chunk_embeddings ADD COLUMN retrieval_text TEXT",
                [],
            );
        }
        let _ = conn.execute(
            "UPDATE chunk_embeddings SET retrieval_text = chunk_text WHERE retrieval_text IS NULL",
            [],
        );
        let _ = conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_chunk_embeddings_layer ON chunk_embeddings(chunk_layer)",
            [],
        );
        Ok(())
    }

    /// Deferred embedding worker: rows flagged when ONNX / embedder is unavailable at ingest time.
    fn migrate_chunk_embeddings_needs_embedding(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        if !Self::column_exists(&conn, "chunk_embeddings", "needs_embedding")? {
            conn.execute(
                "ALTER TABLE chunk_embeddings ADD COLUMN needs_embedding INTEGER NOT NULL DEFAULT 0",
                [],
            )
            .map_err(|e| e.to_string())?;
        }
        let _ = conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_chunk_embeddings_needs_embedding ON chunk_embeddings(needs_embedding) WHERE needs_embedding != 0",
            [],
        );
        Ok(())
    }

    /// Optional start time (seconds) for transcript / media-aligned chunks (temporal RAG).
    fn migrate_chunk_embeddings_timestamp_start(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        if !Self::column_exists(&conn, "chunk_embeddings", "timestamp_start")? {
            conn.execute(
                "ALTER TABLE chunk_embeddings ADD COLUMN timestamp_start REAL",
                [],
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn migrate_rag_documents_source_package(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        // Some startup paths run migrations before the RAG tables have been created (older DBs or
        // maintenance/repair flows). In that case, skip gracefully and let the normal schema/migrations
        // create `rag_documents` first.
        if !Self::table_exists(&conn, "rag_documents")? {
            return Ok(());
        }
        if !Self::column_exists(&conn, "rag_documents", "source_package")? {
            conn.execute(
                "ALTER TABLE rag_documents ADD COLUMN source_package TEXT NOT NULL DEFAULT 'Downloads'",
                [],
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Portable-friendly index key: `relative_path` is unique per row when set (library_root|relpath).
    fn migrate_indexed_files_relative_path(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        if !Self::column_exists(&conn, "indexed_files", "relative_path")? {
            conn.execute(
                "ALTER TABLE indexed_files ADD COLUMN relative_path TEXT",
                [],
            )
            .map_err(|e| e.to_string())?;
            let _ = conn.execute(
                "CREATE UNIQUE INDEX IF NOT EXISTS idx_indexed_files_relative_path ON indexed_files(relative_path) WHERE relative_path IS NOT NULL",
                [],
            );
        }
        Ok(())
    }

    fn table_exists(conn: &Connection, name: &str) -> Result<bool, String> {
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type IN ('table', 'view') AND name = ?1",
                params![name],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        Ok(n > 0)
    }

    /// Phase 2A: FTS5 external-content index over `chunk_embeddings.retrieval_text` (child rows only).
    fn migrate_library_chunks_fts(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let _ = conn.execute("PRAGMA foreign_keys = ON", []);

        conn
            .execute_batch(
                r#"
CREATE VIRTUAL TABLE IF NOT EXISTS library_chunks_fts USING fts5(
  retrieval_text,
  content='chunk_embeddings',
  content_rowid='id'
);

CREATE TRIGGER IF NOT EXISTS chunk_embeddings_library_chunks_fts_ai
AFTER INSERT ON chunk_embeddings
WHEN IFNULL(new.chunk_layer, '') != 'parent'
BEGIN
  INSERT INTO library_chunks_fts(rowid, retrieval_text)
  VALUES (new.id, COALESCE(new.retrieval_text, new.chunk_text, ''));
END;

CREATE TRIGGER IF NOT EXISTS chunk_embeddings_library_chunks_fts_ad
AFTER DELETE ON chunk_embeddings
WHEN IFNULL(old.chunk_layer, '') != 'parent'
BEGIN
  INSERT INTO library_chunks_fts(library_chunks_fts, rowid, retrieval_text)
  VALUES('delete', old.id, COALESCE(old.retrieval_text, old.chunk_text, ''));
END;

CREATE TRIGGER IF NOT EXISTS chunk_embeddings_library_chunks_fts_au
AFTER UPDATE ON chunk_embeddings
BEGIN
  INSERT INTO library_chunks_fts(library_chunks_fts, rowid, retrieval_text)
  SELECT 'delete', old.id, COALESCE(old.retrieval_text, old.chunk_text, '')
  WHERE IFNULL(old.chunk_layer, '') != 'parent';
  INSERT INTO library_chunks_fts(rowid, retrieval_text)
  SELECT new.id, COALESCE(new.retrieval_text, new.chunk_text, '')
  WHERE IFNULL(new.chunk_layer, '') != 'parent';
END;
"#,
            )
            .map_err(|e| e.to_string())?;

        let fts_ok = Self::table_exists(&conn, "library_chunks_fts").unwrap_or(false);
        if !fts_ok {
            return Ok(());
        }

        let fts_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM library_chunks_fts", [], |r| r.get(0))
            .unwrap_or(0);
        let chunk_rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM chunk_embeddings WHERE IFNULL(chunk_layer, '') != 'parent'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if fts_count != chunk_rows {
            let _ = conn.execute("DELETE FROM library_chunks_fts", []);
            let _ = conn.execute(
                "INSERT INTO library_chunks_fts(rowid, retrieval_text) SELECT id, COALESCE(retrieval_text, chunk_text, '') FROM chunk_embeddings WHERE IFNULL(chunk_layer, '') != 'parent'",
                [],
            );
        }

        Ok(())
    }


    /// Recreate sqlite-vec shadow table when dimension changes (256 -> 384 for ONNX MiniLM-class models).
    fn migrate_chunk_embeddings_vec384(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let sql: Option<String> = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type='table' AND name='chunk_embeddings_vec'",
                [],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let needs_recreate = match &sql {
            None => true,
            Some(row) => !row.contains("384"),
        };
        if needs_recreate {
            eprintln!(
                "[athena] sqlite-vec: recreating chunk_embeddings_vec for 384 dimensions (rebuild vector index to repopulate)"
            );
            let _ = conn.execute("DROP TABLE IF EXISTS chunk_embeddings_vec", []);
            conn.execute(
                "CREATE VIRTUAL TABLE IF NOT EXISTS chunk_embeddings_vec USING vec0( embedding float[384] );",
                [],
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Adds columns / tables for installs that predate newer map schema.
    fn migrate_map_schema(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let _ = conn.execute(
            "ALTER TABLE map_markers ADD COLUMN category TEXT NOT NULL DEFAULT 'hazard'",
            [],
        );
        Ok(())
    }

    fn migrate_radio_telemetry_schema(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;

        // Older installs will not have `source_node_id`.
        let mut has_source_node_id = false;
        if let Ok(mut stmt) = conn.prepare("PRAGMA table_info(radio_telemetry)") {
            let rows = stmt.query_map([], |row| {
                // PRAGMA table_info columns: cid, name, type, notnull, dflt_value, pk
                let name: String = row.get(1)?;
                Ok(name)
            });
            if let Ok(rows_iter) = rows {
                for name in rows_iter.flatten() {
                    if name == "source_node_id" {
                        has_source_node_id = true;
                        break;
                    }
                }
            }
        }

        if !has_source_node_id {
            // Add with a default so existing rows are safe.
            let _ = conn.execute(
                "ALTER TABLE radio_telemetry ADD COLUMN source_node_id INTEGER NOT NULL DEFAULT 0",
                [],
            );
        }

        // Ensure composite uniqueness for safe INSERT OR IGNORE from LAN peers.
        let _ = conn.execute(
            "CREATE UNIQUE INDEX IF NOT EXISTS uniq_radio_telemetry_timestamp_source ON radio_telemetry(timestamp, source_node_id)",
            [],
        );

        Ok(())
    }

    /// Adds logbook author / threading columns for older installs (idempotent).
    fn migrate_logbook_schema(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut cols: Vec<String> = Vec::new();
        if let Ok(mut stmt) = conn.prepare("PRAGMA table_info(logbook_entries)") {
            let rows = stmt.query_map([], |row| {
                let name: String = row.get(1)?;
                Ok(name)
            });
            if let Ok(iter) = rows {
                for n in iter.flatten() {
                    cols.push(n);
                }
            }
        }
        if !cols.iter().any(|c| c == "author_name") {
            let _ = conn.execute(
                "ALTER TABLE logbook_entries ADD COLUMN author_name TEXT NOT NULL DEFAULT ''",
                [],
            );
        }
        if !cols.iter().any(|c| c == "author_occupation") {
            let _ = conn.execute(
                "ALTER TABLE logbook_entries ADD COLUMN author_occupation TEXT NOT NULL DEFAULT ''",
                [],
            );
        }
        if !cols.iter().any(|c| c == "parent_entry_id") {
            let _ = conn.execute(
                "ALTER TABLE logbook_entries ADD COLUMN parent_entry_id TEXT REFERENCES logbook_entries(id)",
                [],
            );
        }
        if !cols.iter().any(|c| c == "notebook") {
            let _ = conn.execute(
                "ALTER TABLE logbook_entries ADD COLUMN notebook TEXT NOT NULL DEFAULT 'general'",
                [],
            );
        }
        let _ = conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_logbook_parent_entry_id ON logbook_entries(parent_entry_id)",
            [],
        );
        Ok(())
    }

    /// Roman-tier RBAC personnel, quartermaster inventory, and audit log (idempotent).
    fn migrate_personnel_inventory_schema(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let _ = conn.execute("PRAGMA foreign_keys = ON", []);
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS personnel (
                id TEXT PRIMARY KEY NOT NULL,
                name TEXT NOT NULL UNIQUE,
                division TEXT NOT NULL,
                rank_level INTEGER NOT NULL,
                pin_hash TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                operational_status TEXT NOT NULL DEFAULT 'active'
            );
            CREATE INDEX IF NOT EXISTS idx_personnel_division ON personnel(division);

            CREATE TABLE IF NOT EXISTS inventory (
                id TEXT PRIMARY KEY NOT NULL,
                division TEXT NOT NULL,
                item_name TEXT NOT NULL,
                quantity INTEGER NOT NULL DEFAULT 0,
                last_updated INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_inventory_division ON inventory(division);

            CREATE TABLE IF NOT EXISTS inventory_log (
                id TEXT PRIMARY KEY NOT NULL,
                item_id TEXT NOT NULL,
                user_id TEXT NOT NULL,
                action_type TEXT NOT NULL,
                old_qty INTEGER,
                new_qty INTEGER,
                timestamp INTEGER NOT NULL,
                caesar_fingerprint TEXT,
                FOREIGN KEY (item_id) REFERENCES inventory(id),
                FOREIGN KEY (user_id) REFERENCES personnel(id)
            );
            CREATE INDEX IF NOT EXISTS idx_inventory_log_item ON inventory_log(item_id);
            CREATE INDEX IF NOT EXISTS idx_inventory_log_ts ON inventory_log(timestamp DESC);
            ",
        )
        .map_err(|e| e.to_string())?;

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM personnel", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        if count == 0 {
            use rand::Rng;
            let pin_num: u32 = rand::thread_rng().gen_range(100_000..=999_999);
            let pin_str = format!("{pin_num:06}");
            let hash = personnel_pin_hash(&pin_str);
            let id = uuid::Uuid::new_v4().to_string();
            let now_ms = chrono::Utc::now().timestamp_millis();
            println!(
                "[athena] ROMAN RBAC: default operator Caesar created (division COMMAND, rank 5). SAVE THIS PIN (shown once): {pin_str}"
            );
            conn.execute(
                "INSERT INTO personnel (id, name, division, rank_level, pin_hash, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![id, "Caesar", "COMMAND", 5i64, hash, now_ms],
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Adds personnel.display_name and inventory.category for existing DBs (idempotent).
    fn migrate_roman_rbac_column_updates(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let personnel_cols: Vec<String> = {
            let mut stmt = conn
                .prepare("PRAGMA table_info(personnel)")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |row| row.get::<_, String>(1))
                .map_err(|e| e.to_string())?;
            rows.filter_map(|r| r.ok()).collect()
        };
        if !personnel_cols.iter().any(|c| c == "display_name") {
            conn.execute("ALTER TABLE personnel ADD COLUMN display_name TEXT", [])
                .map_err(|e| e.to_string())?;
        }
        if !personnel_cols.iter().any(|c| c == "operational_status") {
            conn.execute(
                "ALTER TABLE personnel ADD COLUMN operational_status TEXT NOT NULL DEFAULT 'active'",
                [],
            )
            .map_err(|e| e.to_string())?;
        }
        let inv_cols: Vec<String> = {
            let mut stmt = conn
                .prepare("PRAGMA table_info(inventory)")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |row| row.get::<_, String>(1))
                .map_err(|e| e.to_string())?;
            rows.filter_map(|r| r.ok()).collect()
        };
        if !inv_cols.is_empty() && !inv_cols.iter().any(|c| c == "category") {
            conn.execute(
                "ALTER TABLE inventory ADD COLUMN category TEXT NOT NULL DEFAULT 'general'",
                [],
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Logistics Library: adds submitter metadata to logbook_entries and tables for AI annotations and inventory proposals.
    fn migrate_logistics_library_schema(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;

        // Extend logbook_entries with authenticated submitter metadata (idempotent).
        let cols: Vec<String> = {
            let mut stmt = conn
                .prepare("PRAGMA table_info(logbook_entries)")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |row| row.get::<_, String>(1))
                .map_err(|e| e.to_string())?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r.map_err(|e| e.to_string())?);
            }
            out
        };
        if !cols.iter().any(|c| c == "user_id") {
            let _ = conn.execute("ALTER TABLE logbook_entries ADD COLUMN user_id TEXT", []);
        }
        if !cols.iter().any(|c| c == "author_rank_level") {
            let _ = conn.execute(
                "ALTER TABLE logbook_entries ADD COLUMN author_rank_level INTEGER",
                [],
            );
        }
        if !cols.iter().any(|c| c == "author_division") {
            let _ = conn.execute("ALTER TABLE logbook_entries ADD COLUMN author_division TEXT", []);
        }
        let _ = conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_logbook_user_id ON logbook_entries(user_id)",
            [],
        );
        let _ = conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_logbook_rank_level ON logbook_entries(author_rank_level)",
            [],
        );
        let _ = conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_logbook_division ON logbook_entries(author_division)",
            [],
        );

        // New tables (idempotent).
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS logistics_annotations (
                logbook_entry_id TEXT PRIMARY KEY NOT NULL,
                section TEXT NOT NULL,
                ai_json TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'pending',
                created_at INTEGER NOT NULL,
                FOREIGN KEY (logbook_entry_id) REFERENCES logbook_entries(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_logistics_annotations_section ON logistics_annotations(section);
            CREATE INDEX IF NOT EXISTS idx_logistics_annotations_created_at ON logistics_annotations(created_at DESC);

            CREATE TABLE IF NOT EXISTS inventory_proposals (
                id TEXT PRIMARY KEY NOT NULL,
                logbook_entry_id TEXT NOT NULL,
                submitted_by_user_id TEXT,
                submitted_at INTEGER NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending',
                applied_by_user_id TEXT,
                applied_at INTEGER,
                FOREIGN KEY (logbook_entry_id) REFERENCES logbook_entries(id) ON DELETE CASCADE,
                FOREIGN KEY (submitted_by_user_id) REFERENCES personnel(id),
                FOREIGN KEY (applied_by_user_id) REFERENCES personnel(id)
            );
            CREATE INDEX IF NOT EXISTS idx_inventory_proposals_entry ON inventory_proposals(logbook_entry_id);
            CREATE INDEX IF NOT EXISTS idx_inventory_proposals_status ON inventory_proposals(status);
            CREATE INDEX IF NOT EXISTS idx_inventory_proposals_submitted_at ON inventory_proposals(submitted_at DESC);

            CREATE TABLE IF NOT EXISTS inventory_proposal_items (
                id TEXT PRIMARY KEY NOT NULL,
                proposal_id TEXT NOT NULL,
                action TEXT NOT NULL,
                division TEXT NOT NULL,
                item_name TEXT NOT NULL,
                category TEXT NOT NULL DEFAULT 'general',
                quantity INTEGER NOT NULL DEFAULT 0,
                confidence REAL,
                FOREIGN KEY (proposal_id) REFERENCES inventory_proposals(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_inventory_proposal_items_proposal ON inventory_proposal_items(proposal_id);
            ",
        )
        .map_err(|e| e.to_string())?;

        Ok(())
    }


    /// Mesh SOS columns on `mesh_profiles` and expanded `mesh_chats` message types (idempotent).
    fn migrate_mesh_sos_schema(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let profile_cols: Vec<String> = {
            let mut stmt = conn
                .prepare("PRAGMA table_info(mesh_profiles)")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |row| row.get::<_, String>(1))
                .map_err(|e| e.to_string())?;
            rows.filter_map(|r| r.ok()).collect()
        };
        if !profile_cols.iter().any(|c| c == "sos_state") {
            conn.execute(
                "ALTER TABLE mesh_profiles ADD COLUMN sos_state INTEGER NOT NULL DEFAULT 0",
                [],
            )
            .map_err(|e| e.to_string())?;
        }
        if !profile_cols.iter().any(|c| c == "is_registered") {
            conn.execute(
                "ALTER TABLE mesh_profiles ADD COLUMN is_registered INTEGER NOT NULL DEFAULT 1",
                [],
            )
            .map_err(|e| e.to_string())?;
        }
        if !profile_cols.iter().any(|c| c == "rank_level") {
            conn.execute(
                "ALTER TABLE mesh_profiles ADD COLUMN rank_level INTEGER NOT NULL DEFAULT 1",
                [],
            )
            .map_err(|e| e.to_string())?;
        }
        let tbl_sql: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type='table' AND name='mesh_chats'",
                [],
                |r| r.get(0),
            )
            .unwrap_or_default();
        if !tbl_sql.contains("'clearance'") {
            conn.execute_batch(
                "
                CREATE TABLE mesh_chats_new (
                    id TEXT PRIMARY KEY NOT NULL,
                    node_id INTEGER NOT NULL,
                    sender_type TEXT NOT NULL CHECK (sender_type IN ('human','ai','remote')),
                    message_type TEXT NOT NULL CHECK (message_type IN ('auth','chat','prompt','response','operator','emergency','clearance')),
                    content TEXT NOT NULL,
                    created_at INTEGER NOT NULL
                );
                INSERT INTO mesh_chats_new SELECT * FROM mesh_chats;
                DROP TABLE mesh_chats;
                ALTER TABLE mesh_chats_new RENAME TO mesh_chats;
                CREATE INDEX IF NOT EXISTS idx_mesh_chats_node_created ON mesh_chats(node_id, created_at ASC);
                ",
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Saved places, route blockages, AI staging, and Valhalla routing region metadata.
    fn migrate_map_overhaul_schema(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS saved_places (
                id TEXT PRIMARY KEY NOT NULL,
                name TEXT NOT NULL,
                lat REAL NOT NULL,
                lng REAL NOT NULL,
                icon TEXT NOT NULL DEFAULT 'pin',
                color TEXT NOT NULL DEFAULT '#3388ff',
                scope TEXT NOT NULL CHECK (scope IN ('personal','shared')),
                owner_personnel_id TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                deleted_at TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_saved_places_updated ON saved_places(updated_at DESC);
            CREATE INDEX IF NOT EXISTS idx_saved_places_scope ON saved_places(scope);
            CREATE INDEX IF NOT EXISTS idx_saved_places_owner ON saved_places(owner_personnel_id);

            CREATE TABLE IF NOT EXISTS route_blockages (
                id TEXT PRIMARY KEY NOT NULL,
                geometry_json TEXT NOT NULL,
                reason TEXT NOT NULL DEFAULT '',
                severity TEXT NOT NULL CHECK (severity IN ('impassable','slow')),
                modes_affected TEXT NOT NULL DEFAULT 'walk,drive,bike,offroad',
                starts_at TEXT NOT NULL,
                expires_at TEXT,
                status TEXT NOT NULL CHECK (status IN ('pending','active','rejected','expired')),
                proposed_by_personnel_id TEXT NOT NULL,
                approved_by_personnel_id TEXT,
                approved_at TEXT,
                notes TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_route_blockages_updated ON route_blockages(updated_at DESC);
            CREATE INDEX IF NOT EXISTS idx_route_blockages_status ON route_blockages(status);

            CREATE TABLE IF NOT EXISTS ai_staged_changes (
                id TEXT PRIMARY KEY NOT NULL,
                entity_type TEXT NOT NULL,
                action TEXT NOT NULL,
                target_entity_id TEXT,
                payload_json TEXT NOT NULL,
                diff_summary TEXT NOT NULL DEFAULT '',
                source TEXT NOT NULL CHECK (source IN ('ai','user')),
                proposed_by_personnel_id TEXT,
                status TEXT NOT NULL CHECK (status IN ('pending','approved','rejected','auto_approved')),
                reviewed_by_personnel_id TEXT,
                reviewed_at TEXT,
                review_notes TEXT,
                auto_approve_rule_id TEXT,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_ai_staged_status ON ai_staged_changes(status, created_at DESC);

            CREATE TABLE IF NOT EXISTS ai_autoapprove_rules (
                id TEXT PRIMARY KEY NOT NULL,
                entity_type TEXT NOT NULL,
                action TEXT NOT NULL,
                predicate_json TEXT NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1,
                created_by_personnel_id TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS routing_regions (
                id TEXT PRIMARY KEY NOT NULL,
                name TEXT NOT NULL,
                bbox_json TEXT NOT NULL DEFAULT '{}',
                source_pbf_path TEXT,
                tiles_path TEXT NOT NULL DEFAULT '',
                build_status TEXT NOT NULL DEFAULT 'queued',
                modes_built TEXT NOT NULL DEFAULT '',
                size_bytes INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL
            );
            ",
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Tactical area-of-control (territory) polygons for map overlay and routing avoidance.
    fn migrate_territories_schema(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS territories (
                id TEXT PRIMARY KEY NOT NULL,
                name TEXT NOT NULL,
                status TEXT NOT NULL CHECK (status IN ('friendly','enemy','disputed','unexplored')),
                color TEXT NOT NULL,
                geometry_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                created_by_personnel_id TEXT NOT NULL,
                deleted_at TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_territories_status ON territories(status);
            CREATE INDEX IF NOT EXISTS idx_territories_updated ON territories(updated_at DESC);
            ",
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Directorates, intake dedupe, map context on logbook, proposal line locations.
    fn migrate_directorate_intake_schema(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let personnel_cols: Vec<String> = {
            let mut stmt = conn
                .prepare("PRAGMA table_info(personnel)")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |row| row.get::<_, String>(1))
                .map_err(|e| e.to_string())?;
            rows.filter_map(|r| r.ok()).collect()
        };
        if !personnel_cols.iter().any(|c| c == "directorate") {
            conn.execute("ALTER TABLE personnel ADD COLUMN directorate TEXT", [])
                .map_err(|e| e.to_string())?;
        }
        if !personnel_cols.iter().any(|c| c == "tier") {
            conn.execute("ALTER TABLE personnel ADD COLUMN tier TEXT", [])
                .map_err(|e| e.to_string())?;
        }
        if !personnel_cols.iter().any(|c| c == "job_title") {
            conn.execute("ALTER TABLE personnel ADD COLUMN job_title TEXT", [])
                .map_err(|e| e.to_string())?;
        }
        let _ = conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_personnel_directorate ON personnel(directorate)",
            [],
        );

        let inv_cols: Vec<String> = {
            let mut stmt = conn
                .prepare("PRAGMA table_info(inventory)")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |row| row.get::<_, String>(1))
                .map_err(|e| e.to_string())?;
            rows.filter_map(|r| r.ok()).collect()
        };
        if !inv_cols.iter().any(|c| c == "directorate") {
            conn.execute("ALTER TABLE inventory ADD COLUMN directorate TEXT", [])
                .map_err(|e| e.to_string())?;
        }
        let _ = conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_inventory_directorate ON inventory(directorate)",
            [],
        );

        // Heuristic backfill: map legacy `division` text to directorate key.
        let _ = conn.execute(
            "UPDATE inventory SET directorate = 'InformationMedical' WHERE directorate IS NULL AND (
                 lower(division) LIKE '%medical%' OR lower(division) LIKE '%med%'
                 OR lower(division) LIKE '%academy%' OR lower(division) LIKE '%archive%'
                 OR lower(division) LIKE '%intel%')",
            [],
        );
        let _ = conn.execute(
            "UPDATE inventory SET directorate = 'Vanguard' WHERE directorate IS NULL AND (
                 lower(division) LIKE '%tacti%' OR lower(division) LIKE '%security%'
                 OR lower(division) LIKE '%armo%' OR lower(division) LIKE '%perimeter%'
                 OR lower(division) LIKE '%recon%')",
            [],
        );
        let _ = conn.execute(
            "UPDATE inventory SET directorate = 'Sustenance' WHERE directorate IS NULL AND (
                 lower(division) LIKE '%food%' OR lower(division) LIKE '%farm%'
                 OR lower(division) LIKE '%agri%' OR lower(division) LIKE '%water%'
                 OR lower(division) LIKE '%forest%' OR lower(division) LIKE '%cult%')",
            [],
        );
        let _ = conn.execute(
            "UPDATE inventory SET directorate = 'Infrastructure' WHERE directorate IS NULL",
            [],
        );

        for (col, sql) in [
            ("map_lat", "ALTER TABLE logbook_entries ADD COLUMN map_lat REAL"),
            ("map_lon", "ALTER TABLE logbook_entries ADD COLUMN map_lon REAL"),
            (
                "saved_place_id",
                "ALTER TABLE logbook_entries ADD COLUMN saved_place_id TEXT",
            ),
            (
                "location_text",
                "ALTER TABLE logbook_entries ADD COLUMN location_text TEXT",
            ),
            (
                "author_directorate",
                "ALTER TABLE logbook_entries ADD COLUMN author_directorate TEXT",
            ),
            (
                "author_tier",
                "ALTER TABLE logbook_entries ADD COLUMN author_tier TEXT",
            ),
            (
                "author_job_title",
                "ALTER TABLE logbook_entries ADD COLUMN author_job_title TEXT",
            ),
        ] {
            let lb_cols: Vec<String> = {
                let mut stmt = conn
                    .prepare("PRAGMA table_info(logbook_entries)")
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map([], |row| row.get::<_, String>(1))
                    .map_err(|e| e.to_string())?;
                rows.filter_map(|r| r.ok()).collect()
            };
            if !lb_cols.iter().any(|c| c == col) {
                conn.execute(sql, []).map_err(|e| e.to_string())?;
            }
        }

        let prop_item_cols: Vec<String> = {
            let mut stmt = conn
                .prepare("PRAGMA table_info(inventory_proposal_items)")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |row| row.get::<_, String>(1))
                .map_err(|e| e.to_string())?;
            rows.filter_map(|r| r.ok()).collect()
        };
        if !prop_item_cols.is_empty() && !prop_item_cols.iter().any(|c| c == "line_location") {
            conn.execute(
                "ALTER TABLE inventory_proposal_items ADD COLUMN line_location TEXT",
                [],
            )
            .map_err(|e| e.to_string())?;
        }

        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS inventory_intake_events (
                id TEXT PRIMARY KEY NOT NULL,
                logbook_entry_id TEXT NOT NULL,
                inventory_id TEXT,
                directorate TEXT NOT NULL,
                item_norm TEXT NOT NULL,
                location_norm TEXT,
                delta_qty INTEGER NOT NULL,
                submitter_user_id TEXT,
                applied INTEGER NOT NULL DEFAULT 1,
                reason TEXT,
                other_user_id TEXT,
                created_at INTEGER NOT NULL,
                FOREIGN KEY (logbook_entry_id) REFERENCES logbook_entries(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_intake_dedupe ON inventory_intake_events (directorate, item_norm, location_norm, created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_intake_submitter ON inventory_intake_events (submitter_user_id, created_at DESC);
            ",
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Adds `athena_architect`, bootstraps the designer account once (PIN + rank V), and keeps the badge exclusive.
    fn migrate_personnel_architect_badge(&self) -> Result<(), String> {
        const META_KEY: &str = "personnel_architect_pin_seeded_v1";
        const ARCH_PIN_ONCE: &str = "000000";
        const LEGACY_BOOTSTRAP_LOGIN: &str = "StationArchitect";
        const BOOTSTRAP_DISPLAY_NAME: &str = "the ATHENA developer";

        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS app_meta (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL);",
        )
        .map_err(|e| e.to_string())?;

        let personnel_cols: Vec<String> = {
            let mut stmt = conn
                .prepare("PRAGMA table_info(personnel)")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |row| row.get::<_, String>(1))
                .map_err(|e| e.to_string())?;
            rows.filter_map(|r| r.ok()).collect()
        };
        if !personnel_cols.iter().any(|c| c == "athena_architect") {
            conn.execute(
                "ALTER TABLE personnel ADD COLUMN athena_architect INTEGER NOT NULL DEFAULT 0",
                [],
            )
            .map_err(|e| e.to_string())?;
        }
        let _ = conn.execute(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_personnel_single_architect ON personnel(athena_architect) WHERE athena_architect = 1",
            [],
        );

        let seeded: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM app_meta WHERE key = ?1",
                params![META_KEY],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;

        conn.execute("UPDATE personnel SET athena_architect = 0", [])
            .map_err(|e| e.to_string())?;

        let hash = personnel_pin_hash(ARCH_PIN_ONCE);
        let now_ms = Utc::now().timestamp_millis();

        if seeded == 0 {
            // First seed: prefer the row that already matches the architect PIN (custom login names),
            // then the canonical login, then create the default row.
            let n = conn
                .execute(
                    "UPDATE personnel SET rank_level = 5, clearance_level = 5, pin_hash = ?1, athena_architect = 1, division = 'COMMAND',
                     display_name = CASE WHEN display_name IS NULL OR trim(display_name) = '' THEN ?2 ELSE display_name END
                     WHERE id = (SELECT id FROM personnel WHERE pin_hash = ?1 ORDER BY created_at ASC LIMIT 1)",
                    params![hash, BOOTSTRAP_DISPLAY_NAME],
                )
                .map_err(|e| e.to_string())?;
            let n = if n == 0 {
                conn.execute(
                    "UPDATE personnel SET rank_level = 5, clearance_level = 5, pin_hash = ?1, athena_architect = 1, division = 'COMMAND',
                     display_name = CASE WHEN display_name IS NULL OR trim(display_name) = '' THEN ?3 ELSE display_name END
                     WHERE lower(name) = lower(?2)",
                    params![hash, PERSONNEL_ARCHITECT_LOGIN, BOOTSTRAP_DISPLAY_NAME],
                )
                .map_err(|e| e.to_string())?
            } else {
                n
            };
            if n == 0 {
                let id = uuid::Uuid::new_v4().to_string();
                conn.execute(
                    "INSERT INTO personnel (id, name, division, rank_level, clearance_level, pin_hash, created_at, display_name, operational_status, directorate, tier, job_title, athena_architect)
                     VALUES (?1, ?2, 'COMMAND', 5, 5, ?3, ?4, ?5, 'active', NULL, NULL, NULL, 1)",
                    params![id, PERSONNEL_ARCHITECT_LOGIN, hash, now_ms, BOOTSTRAP_DISPLAY_NAME],
                )
                .map_err(|e| e.to_string())?;
            }
            conn.execute(
                "INSERT INTO app_meta (key, value) VALUES (?1, '1')",
                params![META_KEY],
            )
            .map_err(|e| e.to_string())?;
        } else {
            // Re-assert a single architect seat on every open: whoever holds the bootstrapped PIN keeps
            // the badge (supports renamed logins); otherwise fall back to the canonical login row.
            let n = conn
                .execute(
                    "UPDATE personnel SET athena_architect = 1, rank_level = 5, clearance_level = 5
                     WHERE id = (SELECT id FROM personnel WHERE pin_hash = ?1 ORDER BY created_at ASC LIMIT 1)",
                    params![hash],
                )
                .map_err(|e| e.to_string())?;
            if n == 0 {
                let _ = conn.execute(
                    "UPDATE personnel SET athena_architect = 1, rank_level = 5, clearance_level = 5
                     WHERE lower(name) = lower(?1)",
                    params![PERSONNEL_ARCHITECT_LOGIN],
                );
            }
        }

        // Older builds used `StationArchitect`; rename to the configured bootstrap login when PIN matches.
        let _ = conn.execute(
            "UPDATE personnel SET name = ?1,
                 display_name = CASE
                   WHEN display_name IS NULL OR trim(display_name) = '' THEN ?2
                   WHEN lower(trim(display_name)) = lower('Station Architect') THEN ?2
                   ELSE display_name END
             WHERE lower(trim(name)) = lower(?3) AND pin_hash = ?4",
            params![
                PERSONNEL_ARCHITECT_LOGIN,
                BOOTSTRAP_DISPLAY_NAME,
                LEGACY_BOOTSTRAP_LOGIN,
                hash,
            ],
        );

        Ok(())
    }

    /// Persistent custom role catalog entries (Domain + level + title).
    fn migrate_role_catalog(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS role_catalog (
                id TEXT PRIMARY KEY NOT NULL,
                domain TEXT NOT NULL,
                level TEXT NOT NULL CHECK (level IN ('l1','l2','l3','l4')),
                title TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_role_catalog_unique ON role_catalog(domain, level, lower(title));
            CREATE INDEX IF NOT EXISTS idx_role_catalog_domain_level ON role_catalog(domain, level, created_at DESC);
            ",
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn list_role_catalog(&self) -> Result<Vec<RoleCatalogEntry>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, domain, level, title, created_at
                 FROM role_catalog
                 ORDER BY domain ASC, level ASC, lower(title) ASC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok(RoleCatalogEntry {
                    id: row.get(0)?,
                    domain: row.get(1)?,
                    level: row.get(2)?,
                    title: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })
            .map_err(|e| e.to_string())?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn add_role_catalog_entry(&self, domain: &str, level: &str, title: &str) -> Result<(), String> {
        let d = domain.trim();
        let l = level.trim();
        let t = title.trim();
        if d.is_empty() || t.is_empty() {
            return Err("Domain and title are required.".to_string());
        }
        if l != "l1" && l != "l2" && l != "l3" && l != "l4" {
            return Err("Invalid role level.".to_string());
        }
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let id = uuid::Uuid::new_v4().to_string();
        let now_ms = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "INSERT OR IGNORE INTO role_catalog (id, domain, level, title, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, d, l, t, now_ms],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn migrate_command_notifications(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn
            .execute_batch(
                "
            CREATE TABLE IF NOT EXISTS command_notifications (
                id TEXT PRIMARY KEY NOT NULL,
                kind TEXT NOT NULL,
                severity TEXT NOT NULL,
                title TEXT NOT NULL,
                body TEXT,
                ref_id TEXT,
                created_at INTEGER NOT NULL,
                ack_by_user_id TEXT,
                ack_at INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_cmd_notif_created
                ON command_notifications(created_at DESC);
            ",
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn migrate_perf_indices_v1(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn
            .execute(
                "CREATE INDEX IF NOT EXISTS idx_inventory_log_user_ts ON inventory_log(user_id, timestamp DESC)",
                [],
            )
            .map_err(|e| e.to_string())?;
        let _ = conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_logbook_entries_user_created ON logbook_entries(user_id, created_at DESC)",
            [],
        );
        Ok(())
    }

    /// Offline place-name / address index (per routing region) for map directions.
    fn migrate_geocode_v1(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn
            .execute_batch(
                "
            CREATE TABLE IF NOT EXISTS geocode_features (
                rowid INTEGER PRIMARY KEY AUTOINCREMENT,
                id TEXT NOT NULL UNIQUE,
                region_id TEXT NOT NULL,
                feature_class TEXT NOT NULL,
                primary_name TEXT,
                alt_names TEXT,
                address_str TEXT,
                city TEXT,
                state TEXT,
                postal TEXT,
                country TEXT,
                lat REAL NOT NULL,
                lon REAL NOT NULL,
                importance REAL NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_geocode_region ON geocode_features(region_id);
            CREATE VIRTUAL TABLE IF NOT EXISTS geocode_fts USING fts5(
                primary_name,
                alt_names,
                address_str,
                city,
                content='geocode_features',
                content_rowid='rowid'
            );
            ",
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn migrate_routing_region_geocode_meta(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let cols: Vec<String> = {
            let mut stmt = conn
                .prepare("PRAGMA table_info(routing_regions)")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |row| row.get::<_, String>(1))
                .map_err(|e| e.to_string())?;
            rows.filter_map(|r| r.ok()).collect()
        };
        if !cols.iter().any(|c| c == "geocode_built_at") {
            conn.execute(
                "ALTER TABLE routing_regions ADD COLUMN geocode_built_at TEXT",
                [],
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn command_notification_insert(
        &self,
        kind: &str,
        severity: &str,
        title: &str,
        body: Option<&str>,
        ref_id: Option<&str>,
    ) -> Result<CommandNotificationRow, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let now_ms = chrono::Utc::now().timestamp_millis();
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn
            .execute(
                "INSERT INTO command_notifications (id, kind, severity, title, body, ref_id, created_at, ack_by_user_id, ack_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, NULL)",
                params![id, kind, severity, title, body, ref_id, now_ms],
            )
            .map_err(|e| e.to_string())?;
        self.command_notification_get_by_id(&id)?
            .ok_or_else(|| "Failed to read new notification row.".to_string())
    }

    fn command_notification_get_by_id(&self, id: &str) -> Result<Option<CommandNotificationRow>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, kind, severity, title, body, ref_id, created_at, ack_by_user_id, ack_at
                 FROM command_notifications WHERE id = ?1",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query_map(params![id], |row| {
                Ok(CommandNotificationRow {
                    id: row.get(0)?,
                    kind: row.get(1)?,
                    severity: row.get(2)?,
                    title: row.get(3)?,
                    body: row.get(4)?,
                    ref_id: row.get(5)?,
                    created_at: row.get(6)?,
                    ack_by_user_id: row.get(7)?,
                    ack_at: row.get(8)?,
                })
            })
            .map_err(|e| e.to_string())?;
        match rows.nth(0) {
            Some(Ok(r)) => Ok(Some(r)),
            Some(Err(e)) => Err(e.to_string()),
            None => Ok(None),
        }
    }

    pub fn list_command_notifications(&self, limit: usize) -> Result<Vec<CommandNotificationRow>, String> {
        let take = limit.clamp(1, 500);
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, kind, severity, title, body, ref_id, created_at, ack_by_user_id, ack_at
                 FROM command_notifications
                 ORDER BY created_at DESC
                 LIMIT ?1",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![take as i64], |row| {
                Ok(CommandNotificationRow {
                    id: row.get(0)?,
                    kind: row.get(1)?,
                    severity: row.get(2)?,
                    title: row.get(3)?,
                    body: row.get(4)?,
                    ref_id: row.get(5)?,
                    created_at: row.get(6)?,
                    ack_by_user_id: row.get(7)?,
                    ack_at: row.get(8)?,
                })
            })
            .map_err(|e| e.to_string())?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn count_unacked_command_notifications(&self) -> Result<i64, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let c: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM command_notifications WHERE ack_at IS NULL",
                [],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        Ok(c)
    }

    pub fn ack_command_notification(&self, notif_id: &str, user_id: &str) -> Result<bool, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let now_ms = chrono::Utc::now().timestamp_millis();
        let n = conn
            .execute(
                "UPDATE command_notifications SET ack_by_user_id = ?2, ack_at = ?3
                 WHERE id = ?1 AND ack_at IS NULL",
                params![notif_id, user_id, now_ms],
            )
            .map_err(|e| e.to_string())?;
        Ok(n > 0)
    }

    pub fn ack_all_command_notifications(&self, user_id: &str) -> Result<usize, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let now_ms = chrono::Utc::now().timestamp_millis();
        let n = conn
            .execute(
                "UPDATE command_notifications SET ack_by_user_id = ?1, ack_at = ?2
                 WHERE ack_at IS NULL",
                params![user_id, now_ms],
            )
            .map_err(|e| e.to_string())?;
        Ok(n)
    }

    /// Adds hierarchical registry columns for flexible 5-tier org structure (strings, not enums).
    fn migrate_personnel_hierarchy_v5(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let personnel_cols: Vec<String> = {
            let mut stmt = conn
                .prepare("PRAGMA table_info(personnel)")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |row| row.get::<_, String>(1))
                .map_err(|e| e.to_string())?;
            rows.filter_map(|r| r.ok()).collect()
        };
        if !personnel_cols.iter().any(|c| c == "domain") {
            conn.execute("ALTER TABLE personnel ADD COLUMN domain TEXT", [])
                .map_err(|e| e.to_string())?;
        }
        if !personnel_cols.iter().any(|c| c == "role_l1") {
            conn.execute("ALTER TABLE personnel ADD COLUMN role_l1 TEXT", [])
                .map_err(|e| e.to_string())?;
        }
        if !personnel_cols.iter().any(|c| c == "role_l2") {
            conn.execute("ALTER TABLE personnel ADD COLUMN role_l2 TEXT", [])
                .map_err(|e| e.to_string())?;
        }
        if !personnel_cols.iter().any(|c| c == "role_l3") {
            conn.execute("ALTER TABLE personnel ADD COLUMN role_l3 TEXT", [])
                .map_err(|e| e.to_string())?;
        }
        if !personnel_cols.iter().any(|c| c == "role_l4") {
            conn.execute("ALTER TABLE personnel ADD COLUMN role_l4 TEXT", [])
                .map_err(|e| e.to_string())?;
        }

        // Best-effort backfill from previous (directorate/tier/job_title/division) structure.
        // This preserves existing data without enforcing any rigid enum mapping.
        let _ = conn.execute(
            "UPDATE personnel SET role_l3 = directorate WHERE role_l3 IS NULL AND directorate IS NOT NULL AND trim(directorate) != ''",
            [],
        );
        let _ = conn.execute(
            "UPDATE personnel SET role_l2 = job_title WHERE role_l2 IS NULL AND tier = 'sector_lead' AND job_title IS NOT NULL AND trim(job_title) != ''",
            [],
        );
        let _ = conn.execute(
            "UPDATE personnel SET role_l1 = job_title WHERE role_l1 IS NULL AND (tier IS NULL OR tier != 'sector_lead') AND job_title IS NOT NULL AND trim(job_title) != ''",
            [],
        );
        let _ = conn.execute(
            "UPDATE personnel SET domain = division WHERE domain IS NULL AND division IS NOT NULL AND trim(division) != ''",
            [],
        );

        Ok(())
    }

    /// Adds `clearance_level` (0–5) and backfills from `rank_level` for existing installs.
    fn migrate_personnel_clearance_level(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let personnel_cols: Vec<String> = {
            let mut stmt = conn
                .prepare("PRAGMA table_info(personnel)")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |row| row.get::<_, String>(1))
                .map_err(|e| e.to_string())?;
            rows.filter_map(|r| r.ok()).collect()
        };
        if !personnel_cols.iter().any(|c| c == "clearance_level") {
            conn.execute(
                "ALTER TABLE personnel ADD COLUMN clearance_level INTEGER NOT NULL DEFAULT 1",
                [],
            )
            .map_err(|e| e.to_string())?;
        }
        // One-time backfill: align clearance with current rank.
        let _ = conn.execute("UPDATE personnel SET clearance_level = rank_level", []);
        Ok(())
    }

    pub fn personnel_fetch_by_name(&self, name: &str) -> Result<Option<PersonnelRow>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let name_t = name.trim();
        if name_t.is_empty() {
            return Ok(None);
        }
        let mut stmt = conn
            .prepare(
                "SELECT id, name, division, rank_level, clearance_level, pin_hash, display_name, operational_status, directorate, tier, job_title, athena_architect, domain, role_l1, role_l2, role_l3, role_l4 FROM personnel WHERE LOWER(name) = LOWER(?1) LIMIT 1",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query_map(params![name_t], |row| {
                Ok(PersonnelRow {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    division: row.get(2)?,
                    rank_level: row.get(3)?,
                    clearance_level: row.get(4)?,
                    pin_hash: row.get(5)?,
                    display_name: row.get(6)?,
                    operational_status: row.get(7)?,
                    directorate: row.get(8)?,
                    tier: row.get(9)?,
                    job_title: row.get(10)?,
                    athena_architect: row.get::<_, i64>(11)? as i32,
                    domain: row.get(12)?,
                    role_l1: row.get(13)?,
                    role_l2: row.get(14)?,
                    role_l3: row.get(15)?,
                    role_l4: row.get(16)?,
                })
            })
            .map_err(|e| e.to_string())?;
        match rows.next() {
            None => Ok(None),
            Some(r) => Ok(Some(r.map_err(|e| e.to_string())?)),
        }
    }

    pub fn personnel_list_public(&self) -> Result<Vec<PersonnelPublic>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, name, division, rank_level, clearance_level, created_at, display_name, operational_status, directorate, tier, job_title, athena_architect, domain, role_l1, role_l2, role_l3, role_l4 FROM personnel ORDER BY name COLLATE NOCASE",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok(PersonnelPublic {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    division: row.get(2)?,
                    rank_level: row.get(3)?,
                    clearance_level: row.get(4)?,
                    created_at: row.get(5)?,
                    display_name: row.get(6)?,
                    operational_status: row.get(7)?,
                    directorate: row.get(8)?,
                    tier: row.get(9)?,
                    job_title: row.get(10)?,
                    athena_architect: row.get::<_, i64>(11)? != 0,
                    domain: row.get(12)?,
                    role_l1: row.get(13)?,
                    role_l2: row.get(14)?,
                    role_l3: row.get(15)?,
                    role_l4: row.get(16)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn personnel_insert(
        &self,
        id: &str,
        name: &str,
        division: &str,
        rank_level: i32,
        pin_hash: &str,
        display_name: Option<&str>,
        directorate: Option<&str>,
        tier: Option<&str>,
        job_title: Option<&str>,
        athena_architect: i32,
        domain: Option<&str>,
        role_l1: Option<&str>,
        role_l2: Option<&str>,
        role_l3: Option<&str>,
        role_l4: Option<&str>,
    ) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let now_ms = Utc::now().timestamp_millis();
        let disp: Option<String> = display_name
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let dir: Option<String> = directorate
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let t: Option<String> = tier
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let jt: Option<String> = job_title
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let dom: Option<String> = domain
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let rl1: Option<String> = role_l1
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let rl2: Option<String> = role_l2
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let rl3: Option<String> = role_l3
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let rl4: Option<String> = role_l4
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        conn.execute(
            "INSERT INTO personnel (id, name, division, rank_level, clearance_level, pin_hash, created_at, display_name, operational_status, directorate, tier, job_title, athena_architect, domain, role_l1, role_l2, role_l3, role_l4) VALUES (?1, ?2, ?3, ?4, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            params![
                id,
                name.trim(),
                division.trim(),
                rank_level as i64,
                pin_hash,
                now_ms,
                disp,
                "active",
                dir,
                t,
                jt,
                athena_architect as i64,
                dom,
                rl1,
                rl2,
                rl3,
                rl4
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn personnel_count(&self) -> Result<i64, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let c: i64 = conn
            .query_row("SELECT COUNT(*) FROM personnel", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        Ok(c)
    }

    /// True when the roster is empty or still only the single bootstrap architect row ([`PERSONNEL_ARCHITECT_LOGIN`]).
    pub fn oobe_first_account_pending(&self) -> Result<bool, String> {
        let c = self.personnel_count()?;
        if c == 0 {
            return Ok(true);
        }
        if c != 1 {
            return Ok(false);
        }
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let name: String = conn
            .query_row("SELECT name FROM personnel LIMIT 1", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        Ok(name.trim().eq_ignore_ascii_case(PERSONNEL_ARCHITECT_LOGIN))
    }

    /// Deletes the lone bootstrap architect row so OOBE can insert the real first operator.
    pub fn oobe_remove_bootstrap_station_architect_if_present(&self) -> Result<(), String> {
        let c = self.personnel_count()?;
        if c == 0 {
            return Ok(());
        }
        if c != 1 {
            return Err(format!(
                "First account setup requires an empty roster or only the default {} row.",
                PERSONNEL_ARCHITECT_LOGIN
            ));
        }
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let name: String = conn
            .query_row("SELECT name FROM personnel LIMIT 1", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        if !name.trim().eq_ignore_ascii_case(PERSONNEL_ARCHITECT_LOGIN) {
            return Err(format!(
                "First account setup is only available while the roster is empty or only the default {} bootstrap row exists.",
                PERSONNEL_ARCHITECT_LOGIN
            ));
        }
        conn.execute("DELETE FROM personnel", [])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn personnel_delete_by_id(&self, id: &str) -> Result<u64, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let n = conn
            .execute("DELETE FROM personnel WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(n as u64)
    }

    pub fn personnel_set_display_name(&self, id: &str, display_name: Option<&str>) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let disp: Option<String> = display_name
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let n = conn
            .execute(
                "UPDATE personnel SET display_name = ?1 WHERE id = ?2",
                params![disp, id],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("Personnel record not found.".to_string());
        }
        Ok(())
    }

    /// Lock-screen login name; must stay unique case-insensitively among personnel.
    pub fn personnel_set_name(&self, id: &str, new_name: &str) -> Result<(), String> {
        let new_name = new_name.trim();
        if new_name.is_empty() {
            return Err("Login name cannot be empty.".to_string());
        }
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let clash: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM personnel WHERE LOWER(name) = LOWER(?1) AND id != ?2",
                params![new_name, id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if clash > 0 {
            return Err("That login name is already in use.".to_string());
        }
        let n = conn
            .execute(
                "UPDATE personnel SET name = ?1 WHERE id = ?2",
                params![new_name, id],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("Personnel record not found.".to_string());
        }
        Ok(())
    }

    pub fn personnel_set_pin_hash(&self, id: &str, pin_hash: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let n = conn
            .execute(
                "UPDATE personnel SET pin_hash = ?1 WHERE id = ?2",
                params![pin_hash, id],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("Personnel record not found.".to_string());
        }
        Ok(())
    }

    pub fn personnel_list_ids_by_rank_level(&self, rank_level: i32) -> Result<Vec<String>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare("SELECT id FROM personnel WHERE rank_level = ?1")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![rank_level as i64], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn personnel_admin_update(
        &self,
        id: &str,
        login_name: &str,
        display_name: Option<&str>,
        division: &str,
        rank_level: i32,
        operational_status: &str,
        directorate: Option<&str>,
        tier: Option<&str>,
        job_title: Option<&str>,
        domain: Option<&str>,
        role_l1: Option<&str>,
        role_l2: Option<&str>,
        role_l3: Option<&str>,
        role_l4: Option<&str>,
    ) -> Result<(), String> {
        let id_t = id.trim();
        if id_t.is_empty() {
            return Err("Personnel id cannot be empty.".to_string());
        }
        let login_name = login_name.trim();
        if login_name.is_empty() {
            return Err("Login name cannot be empty.".to_string());
        }
        let division = division.trim();
        if division.is_empty() {
            return Err("Division cannot be empty.".to_string());
        }
        if !(1..=5).contains(&rank_level) {
            return Err("Rank level must be Level 1–5.".to_string());
        }
        let st = operational_status.trim().to_ascii_lowercase();
        if st != "active" && st != "on_mission" && st != "mia" && st != "kia" {
            return Err("operational_status must be active, on_mission, mia, or kia.".to_string());
        }

        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let clash: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM personnel WHERE LOWER(name) = LOWER(?1) AND id != ?2",
                params![login_name, id_t],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if clash > 0 {
            return Err("That login name is already in use.".to_string());
        }

        let disp: Option<String> = display_name
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let dir: Option<String> = directorate
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let t: Option<String> = tier
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let jt: Option<String> = job_title
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let dom: Option<String> = domain
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let rl1: Option<String> = role_l1
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let rl2: Option<String> = role_l2
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let rl3: Option<String> = role_l3
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let rl4: Option<String> = role_l4
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        let prev_rank: i32 = conn
            .query_row(
                "SELECT rank_level FROM personnel WHERE id = ?1 LIMIT 1",
                params![id_t],
                |r| r.get::<_, i64>(0).map(|v| v as i32),
            )
            .map_err(|e| e.to_string())?;
        let prev_architect: i32 = conn
            .query_row(
                "SELECT COALESCE(athena_architect, 0) FROM personnel WHERE id = ?1 LIMIT 1",
                params![id_t],
                |r| r.get::<_, i64>(0).map(|v| v as i32),
            )
            .map_err(|e| e.to_string())?;

        if rank_level == 5 && prev_rank < 5 {
            let n5: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM personnel WHERE rank_level = 5 AND id != ?1",
                    params![id_t],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?;
            if n5 >= 2 {
                return Err("At most two Level 5 (Command) operators may exist.".to_string());
            }
        }

        let n = conn
            .execute(
                "UPDATE personnel SET name = ?1, display_name = ?2, division = ?3, rank_level = ?4, clearance_level = ?4, operational_status = ?5, directorate = ?6, tier = ?7, job_title = ?8, athena_architect = ?9, domain = ?10, role_l1 = ?11, role_l2 = ?12, role_l3 = ?13, role_l4 = ?14 WHERE id = ?15",
                params![
                    login_name,
                    disp,
                    division,
                    rank_level as i64,
                    st,
                    dir,
                    t,
                    jt,
                    prev_architect as i64,
                    dom,
                    rl1,
                    rl2,
                    rl3,
                    rl4,
                    id_t
                ],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("Personnel record not found.".to_string());
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub fn personnel_transfer_caesar(
        &self,
        actor_id: &str,
        target_id: &str,
        target_login_name: &str,
        target_display_name: Option<&str>,
        target_division: &str,
        target_operational_status: &str,
    ) -> Result<(), String> {
        let actor_id = actor_id.trim();
        let target_id = target_id.trim();
        if actor_id.is_empty() || target_id.is_empty() {
            return Err("personnel_id is required.".to_string());
        }
        if actor_id == target_id {
            return Err("Cannot transfer Caesar to self.".to_string());
        }

        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|e| e.to_string())?;
        tx.execute_batch("BEGIN IMMEDIATE")
            .map_err(|e| e.to_string())?;

        let caesars: i64 = tx
            .query_row(
                "SELECT COUNT(*) FROM personnel WHERE rank_level = 5",
                [],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if caesars != 1 {
            return Err("Caesar transfer requires exactly one existing Caesar.".to_string());
        }

        let actor_rank: i64 = tx
            .query_row(
                "SELECT rank_level FROM personnel WHERE id = ?1 LIMIT 1",
                params![actor_id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if actor_rank != 5 {
            return Err("Only Caesar may transfer Caesar rank.".to_string());
        }

        let target_exists: i64 = tx
            .query_row(
                "SELECT COUNT(*) FROM personnel WHERE id = ?1",
                params![target_id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if target_exists != 1 {
            return Err("Personnel record not found.".to_string());
        }

        // Demote the actor to Level 4, keeping existing metadata.
        let actor_row = tx
            .query_row(
                "SELECT name, display_name, division, operational_status FROM personnel WHERE id = ?1 LIMIT 1",
                params![actor_id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, Option<String>>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                    ))
                },
            )
            .map_err(|e| e.to_string())?;

        let (actor_name, actor_display, actor_division, actor_status) = actor_row;
        let demote_n = tx
            .execute(
                "UPDATE personnel SET name = ?1, display_name = ?2, division = ?3, rank_level = 4, operational_status = ?4 WHERE id = ?5 AND rank_level = 5",
                params![actor_name, actor_display, actor_division, actor_status, actor_id],
            )
            .map_err(|e| e.to_string())?;
        if demote_n != 1 {
            return Err("Caesar transfer failed (actor demotion).".to_string());
        }

        // Promote the target to Level 5 using supplied values (these will be validated by the caller logic too).
        let st = target_operational_status.trim().to_ascii_lowercase();
        if st != "active" && st != "on_mission" && st != "mia" && st != "kia" {
            return Err("operational_status must be active, on_mission, mia, or kia.".to_string());
        }
        let login_name = target_login_name.trim();
        if login_name.is_empty() {
            return Err("Login name cannot be empty.".to_string());
        }
        let division = target_division.trim();
        if division.is_empty() {
            return Err("Division cannot be empty.".to_string());
        }
        let clash: i64 = tx
            .query_row(
                "SELECT COUNT(*) FROM personnel WHERE LOWER(name) = LOWER(?1) AND id != ?2",
                params![login_name, target_id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if clash > 0 {
            return Err("That login name is already in use.".to_string());
        }
        let disp: Option<String> = target_display_name
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let promote_n = tx
            .execute(
                "UPDATE personnel SET name = ?1, display_name = ?2, division = ?3, rank_level = 5, operational_status = ?4 WHERE id = ?5",
                params![login_name, disp, division, st, target_id],
            )
            .map_err(|e| e.to_string())?;
        if promote_n != 1 {
            return Err("Caesar transfer failed (target promotion).".to_string());
        }

        // Final invariant: exactly one Caesar, and it's the target.
        let final_caesars: Vec<String> = {
            let mut stmt = tx
                .prepare("SELECT id FROM personnel WHERE rank_level = 5")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(|e| e.to_string())?;
            let mut v = Vec::new();
            for r in rows {
                v.push(r.map_err(|e| e.to_string())?);
            }
            v
        };
        if final_caesars.len() != 1 || final_caesars[0] != target_id {
            return Err("Caesar transfer invariant check failed.".to_string());
        }

        tx.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn personnel_fetch_public_by_id(&self, id: &str) -> Result<Option<PersonnelPublic>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, name, division, rank_level, clearance_level, created_at, display_name, operational_status, directorate, tier, job_title, athena_architect, domain, role_l1, role_l2, role_l3, role_l4 FROM personnel WHERE id = ?1 LIMIT 1",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query_map(params![id], |row| {
                Ok(PersonnelPublic {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    division: row.get(2)?,
                    rank_level: row.get(3)?,
                    clearance_level: row.get(4)?,
                    created_at: row.get(5)?,
                    display_name: row.get(6)?,
                    operational_status: row.get(7)?,
                    directorate: row.get(8)?,
                    tier: row.get(9)?,
                    job_title: row.get(10)?,
                    athena_architect: row.get::<_, i64>(11)? != 0,
                    domain: row.get(12)?,
                    role_l1: row.get(13)?,
                    role_l2: row.get(14)?,
                    role_l3: row.get(15)?,
                    role_l4: row.get(16)?,
                })
            })
            .map_err(|e| e.to_string())?;
        match rows.next() {
            None => Ok(None),
            Some(r) => Ok(Some(r.map_err(|e| e.to_string())?)),
        }
    }

    pub fn personnel_fetch_by_id(&self, id: &str) -> Result<Option<PersonnelRow>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, name, division, rank_level, clearance_level, pin_hash, display_name, operational_status, directorate, tier, job_title, athena_architect, domain, role_l1, role_l2, role_l3, role_l4 FROM personnel WHERE id = ?1 LIMIT 1",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query_map(params![id], |row| {
                Ok(PersonnelRow {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    division: row.get(2)?,
                    rank_level: row.get(3)?,
                    clearance_level: row.get(4)?,
                    pin_hash: row.get(5)?,
                    display_name: row.get(6)?,
                    operational_status: row.get(7)?,
                    directorate: row.get(8)?,
                    tier: row.get(9)?,
                    job_title: row.get(10)?,
                    athena_architect: row.get::<_, i64>(11)? as i32,
                    domain: row.get(12)?,
                    role_l1: row.get(13)?,
                    role_l2: row.get(14)?,
                    role_l3: row.get(15)?,
                    role_l4: row.get(16)?,
                })
            })
            .map_err(|e| e.to_string())?;
        match rows.next() {
            None => Ok(None),
            Some(r) => Ok(Some(r.map_err(|e| e.to_string())?)),
        }
    }

    pub fn inventory_list(&self) -> Result<Vec<InventoryItemRow>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, division, item_name, quantity, last_updated, category, directorate FROM inventory ORDER BY category COLLATE NOCASE, coalesce(directorate, division) COLLATE NOCASE, item_name COLLATE NOCASE",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok(InventoryItemRow {
                    id: row.get(0)?,
                    division: row.get(1)?,
                    item_name: row.get(2)?,
                    quantity: row.get(3)?,
                    last_updated: row.get(4)?,
                    category: row.get(5)?,
                    directorate: row.get(6)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn inventory_get(&self, id: &str) -> Result<Option<InventoryItemRow>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, division, item_name, quantity, last_updated, category, directorate FROM inventory WHERE id = ?1 LIMIT 1",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query_map(params![id], |row| {
                Ok(InventoryItemRow {
                    id: row.get(0)?,
                    division: row.get(1)?,
                    item_name: row.get(2)?,
                    quantity: row.get(3)?,
                    last_updated: row.get(4)?,
                    category: row.get(5)?,
                    directorate: row.get(6)?,
                })
            })
            .map_err(|e| e.to_string())?;
        match rows.next() {
            None => Ok(None),
            Some(r) => Ok(Some(r.map_err(|e| e.to_string())?)),
        }
    }

    pub fn inventory_insert(
        &self,
        id: &str,
        division: &str,
        item_name: &str,
        quantity: i64,
        category: &str,
        directorate: Option<&str>,
    ) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let now_ms = Utc::now().timestamp_millis();
        let div = division.trim();
        let dir: Option<String> = directorate
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        conn.execute(
            "INSERT INTO inventory (id, division, item_name, quantity, last_updated, category, directorate) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                id,
                div,
                item_name.trim(),
                quantity,
                now_ms,
                category,
                dir
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Returns previous quantity if the row existed and was updated.
    pub fn inventory_set_quantity(&self, id: &str, new_qty: i64) -> Result<Option<i64>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let old: Option<i64> = conn
            .query_row("SELECT quantity FROM inventory WHERE id = ?1", params![id], |r| {
                r.get(0)
            })
            .optional()
            .map_err(|e| e.to_string())?;
        if old.is_none() {
            return Ok(None);
        }
        let now_ms = Utc::now().timestamp_millis();
        conn.execute(
            "UPDATE inventory SET quantity = ?1, last_updated = ?2 WHERE id = ?3",
            params![new_qty, now_ms, id],
        )
        .map_err(|e| e.to_string())?;
        Ok(old)
    }

    pub fn inventory_set_category(&self, id: &str, category: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let now_ms = Utc::now().timestamp_millis();
        let n = conn
            .execute(
                "UPDATE inventory SET category = ?1, last_updated = ?2 WHERE id = ?3",
                params![category, now_ms, id],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("Inventory item not found.".to_string());
        }
        Ok(())
    }

    pub fn inventory_log_insert(
        &self,
        log_id: &str,
        item_id: &str,
        user_id: &str,
        action_type: &str,
        old_qty: Option<i64>,
        new_qty: Option<i64>,
        timestamp_ms: i64,
    ) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO inventory_log (id, item_id, user_id, action_type, old_qty, new_qty, timestamp, caesar_fingerprint) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL)",
            params![
                log_id,
                item_id,
                user_id,
                action_type,
                old_qty,
                new_qty,
                timestamp_ms
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn inventory_log_list(&self) -> Result<Vec<InventoryLogRow>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, item_id, user_id, action_type, old_qty, new_qty, timestamp, caesar_fingerprint FROM inventory_log ORDER BY timestamp DESC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok(InventoryLogRow {
                    id: row.get(0)?,
                    item_id: row.get(1)?,
                    user_id: row.get(2)?,
                    action_type: row.get(3)?,
                    old_qty: row.get(4)?,
                    new_qty: row.get(5)?,
                    timestamp: row.get(6)?,
                    caesar_fingerprint: row.get(7)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn list_logbook_entries_by_user(&self, user_id: &str, limit: usize) -> Result<Vec<LogbookEntry>, String> {
        let uid = user_id.trim();
        if uid.is_empty() {
            return Ok(Vec::new());
        }
        let cap = limit.clamp(1, 200);
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, body, created_at, from_node_id, author_name, author_occupation, user_id, author_rank_level, author_division, parent_entry_id, notebook
                 FROM logbook_entries
                 WHERE user_id = ?1
                 ORDER BY created_at DESC
                 LIMIT ?2",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![uid, cap as i64], |row| {
                let from_raw: Option<i64> = row.get(3)?;
                Ok(LogbookEntry {
                    id: row.get(0)?,
                    body: row.get(1)?,
                    created_at: row.get(2)?,
                    from_node_id: from_raw.and_then(|n| u32::try_from(n).ok()),
                    author_name: row.get(4).unwrap_or_default(),
                    author_occupation: row.get(5).unwrap_or_default(),
                    user_id: row.get(6)?,
                    author_rank_level: row.get(7)?,
                    author_division: row.get(8)?,
                    author_directorate: None,
                    author_tier: None,
                    author_job_title: None,
                    map_lat: None,
                    map_lon: None,
                    saved_place_id: None,
                    location_text: None,
                    parent_entry_id: row.get(9)?,
                    notebook: Self::logbook_notebook_cell(row, 10)?,
                })
            })
            .map_err(|e| e.to_string())?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn list_inventory_log_by_user(&self, user_id: &str, limit: usize) -> Result<Vec<InventoryLogRow>, String> {
        let uid = user_id.trim();
        if uid.is_empty() {
            return Ok(Vec::new());
        }
        let cap = limit.clamp(1, 400);
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, item_id, user_id, action_type, old_qty, new_qty, timestamp, caesar_fingerprint
                 FROM inventory_log
                 WHERE user_id = ?1
                 ORDER BY timestamp DESC
                 LIMIT ?2",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![uid, cap as i64], |row| {
                Ok(InventoryLogRow {
                    id: row.get(0)?,
                    item_id: row.get(1)?,
                    user_id: row.get(2)?,
                    action_type: row.get(3)?,
                    old_qty: row.get(4)?,
                    new_qty: row.get(5)?,
                    timestamp: row.get(6)?,
                    caesar_fingerprint: row.get(7)?,
                })
            })
            .map_err(|e| e.to_string())?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn list_inventory_intake_events_by_user(
        &self,
        user_id: &str,
        limit: usize,
    ) -> Result<Vec<InventoryIntakeEventRow>, String> {
        let uid = user_id.trim();
        if uid.is_empty() {
            return Ok(Vec::new());
        }
        let cap = limit.clamp(1, 400);
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, logbook_entry_id, inventory_id, directorate, item_norm, location_norm, delta_qty,
                        submitter_user_id, reason, other_user_id, created_at
                 FROM inventory_intake_events
                 WHERE submitter_user_id = ?1
                 ORDER BY created_at DESC
                 LIMIT ?2",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![uid, cap as i64], |row| {
                Ok(InventoryIntakeEventRow {
                    id: row.get(0)?,
                    logbook_entry_id: row.get(1)?,
                    inventory_id: row.get(2)?,
                    directorate: row.get(3)?,
                    item_norm: row.get(4)?,
                    location_norm: row.get(5)?,
                    delta_qty: row.get(6)?,
                    submitter_user_id: row.get(7)?,
                    reason: row.get(8)?,
                    other_user_id: row.get(9)?,
                    created_at: row.get(10)?,
                })
            })
            .map_err(|e| e.to_string())?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn inventory_log_append_fingerprint(
        &self,
        log_id: &str,
        note: &str,
    ) -> Result<(), String> {
        let note = note.trim();
        if note.is_empty() {
            return Err("Correction note is empty.".to_string());
        }
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let n = conn
            .execute(
                "UPDATE inventory_log SET caesar_fingerprint = CASE \
                 WHEN caesar_fingerprint IS NULL OR caesar_fingerprint = '' THEN ?2 \
                 ELSE caesar_fingerprint || char(10) || ?2 END \
                 WHERE id = ?1",
                params![log_id, note],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("Log entry not found.".to_string());
        }
        Ok(())
    }

    pub fn inventory_log_get(&self, log_id: &str) -> Result<Option<InventoryLogRow>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, item_id, user_id, action_type, old_qty, new_qty, timestamp, caesar_fingerprint FROM inventory_log WHERE id = ?1 LIMIT 1",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query_map(params![log_id], |row| {
                Ok(InventoryLogRow {
                    id: row.get(0)?,
                    item_id: row.get(1)?,
                    user_id: row.get(2)?,
                    action_type: row.get(3)?,
                    old_qty: row.get(4)?,
                    new_qty: row.get(5)?,
                    timestamp: row.get(6)?,
                    caesar_fingerprint: row.get(7)?,
                })
            })
            .map_err(|e| e.to_string())?;
        match rows.next() {
            None => Ok(None),
            Some(r) => Ok(Some(r.map_err(|e| e.to_string())?)),
        }
    }

    pub fn inventory_find_by_division_item(
        &self,
        division: &str,
        item_name: &str,
    ) -> Result<Option<InventoryItemRow>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, division, item_name, quantity, last_updated, category, directorate
                 FROM inventory
                 WHERE lower(trim(division)) = lower(trim(?1))
                   AND lower(trim(item_name)) = lower(trim(?2))
                 LIMIT 1",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query_map(params![division, item_name], |row| {
                Ok(InventoryItemRow {
                    id: row.get(0)?,
                    division: row.get(1)?,
                    item_name: row.get(2)?,
                    quantity: row.get(3)?,
                    last_updated: row.get(4)?,
                    category: row.get(5)?,
                    directorate: row.get(6)?,
                })
            })
            .map_err(|e| e.to_string())?;
        match rows.next() {
            None => Ok(None),
            Some(r) => Ok(Some(r.map_err(|e| e.to_string())?)),
        }
    }

    /// Find inventory row by directorate key and item (case-insensitive). Prefers `directorate` column; falls back to `division` match.
    pub fn inventory_find_by_directorate_item(
        &self,
        directorate: &str,
        item_name: &str,
    ) -> Result<Option<InventoryItemRow>, String> {
        let d = directorate.trim();
        if d.is_empty() {
            return Ok(None);
        }
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, division, item_name, quantity, last_updated, category, directorate
                 FROM inventory
                 WHERE lower(trim(item_name)) = lower(trim(?1))
                   AND (
                     (directorate IS NOT NULL AND lower(trim(directorate)) = lower(trim(?2)))
                     OR (directorate IS NULL AND lower(trim(division)) = lower(trim(?2)))
                   )
                 LIMIT 1",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query_map(params![item_name, d], |row| {
                Ok(InventoryItemRow {
                    id: row.get(0)?,
                    division: row.get(1)?,
                    item_name: row.get(2)?,
                    quantity: row.get(3)?,
                    last_updated: row.get(4)?,
                    category: row.get(5)?,
                    directorate: row.get(6)?,
                })
            })
            .map_err(|e| e.to_string())?;
        match rows.next() {
            None => Ok(None),
            Some(r) => Ok(Some(r.map_err(|e| e.to_string())?)),
        }
    }

    /// Re-bucket an inventory row to a new directorate key (and division label) — rank 4+ UI.
    pub fn inventory_set_directorate(
        &self,
        item_id: &str,
        new_directorate: &str,
    ) -> Result<(), String> {
        let d = new_directorate.trim();
        if d.is_empty() {
            return Err("directorate is required.".to_string());
        }
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let now_ms = Utc::now().timestamp_millis();
        let n = conn
            .execute(
                "UPDATE inventory SET directorate = ?1, division = ?1, last_updated = ?2 WHERE id = ?3",
                params![d, now_ms, item_id],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("Inventory item not found.".to_string());
        }
        Ok(())
    }

    pub fn inventory_intake_event_insert(
        &self,
        id: &str,
        logbook_entry_id: &str,
        inventory_id: Option<&str>,
        directorate: &str,
        item_norm: &str,
        location_norm: Option<&str>,
        delta_qty: i64,
        submitter_user_id: Option<&str>,
        applied: i32,
        reason: &str,
        other_user_id: Option<&str>,
        created_at: i64,
    ) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO inventory_intake_events (id, logbook_entry_id, inventory_id, directorate, item_norm, location_norm, delta_qty, submitter_user_id, applied, reason, other_user_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                id,
                logbook_entry_id,
                inventory_id,
                directorate.trim(),
                item_norm,
                location_norm,
                delta_qty,
                submitter_user_id,
                applied,
                reason,
                other_user_id,
                created_at
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Another user's successful intake for same (directorate, item, location) within the lookback window.
    pub fn intake_find_duplicate_other_user(
        &self,
        directorate: &str,
        item_norm: &str,
        location_norm: Option<&str>,
        not_before_ms: i64,
        current_user_id: &str,
    ) -> Result<Option<(String, i64)>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let loc = location_norm.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        let mut stmt = conn
            .prepare(
                "SELECT submitter_user_id, created_at FROM inventory_intake_events
                 WHERE applied = 1
                   AND reason = 'auto'
                   AND directorate = ?1
                   AND item_norm = ?2
                   AND COALESCE(location_norm, '') = ?3
                   AND created_at >= ?4
                   AND submitter_user_id IS NOT NULL
                   AND submitter_user_id != ?5
                 ORDER BY created_at DESC
                 LIMIT 1",
            )
            .map_err(|e| e.to_string())?;
        let key_loc = loc.as_deref().unwrap_or("");
        let row = stmt.query_row(
            params![
                directorate,
                item_norm,
                key_loc,
                not_before_ms,
                current_user_id
            ],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
        );
        match row {
            Ok(t) => Ok(Some(t)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn count_pending_proposals_for_submitter(&self, user_id: &str) -> Result<i64, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM inventory_proposals WHERE submitted_by_user_id = ?1 AND status = 'pending'",
                params![user_id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        Ok(n)
    }

    pub fn count_pending_add_item_proposals_for_submitter(&self, user_id: &str) -> Result<i64, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM inventory_proposal_items i
                 JOIN inventory_proposals p ON p.id = i.proposal_id
                 WHERE p.submitted_by_user_id = ?1 AND p.status = 'pending' AND lower(trim(i.action)) = 'add_item'",
                params![user_id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        Ok(n)
    }

    pub fn count_suppressed_intakes_for_submitter(
        &self,
        user_id: &str,
        not_before_ms: i64,
    ) -> Result<i64, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM inventory_intake_events
                 WHERE submitter_user_id = ?1 AND applied = 0 AND reason = 'suppressed_duplicate' AND created_at >= ?2",
                params![user_id, not_before_ms],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        Ok(n)
    }

    pub fn personnel_name_by_id(&self, id: &str) -> Result<Option<String>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let n: Option<String> = conn
            .query_row("SELECT name FROM personnel WHERE id = ?1", params![id], |r| r.get(0))
            .optional()
            .map_err(|e| e.to_string())?;
        Ok(n)
    }

    pub fn logistics_annotation_upsert(
        &self,
        entry_id: &str,
        section: &str,
        ai_json: &str,
        status: &str,
        created_at: i64,
    ) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO logistics_annotations (logbook_entry_id, section, ai_json, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(logbook_entry_id) DO UPDATE SET section=excluded.section, ai_json=excluded.ai_json, status=excluded.status",
            params![entry_id, section.trim(), ai_json, status, created_at],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn inventory_proposal_create(
        &self,
        proposal_id: &str,
        entry_id: &str,
        submitted_by_user_id: Option<&str>,
        submitted_at: i64,
    ) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO inventory_proposals (id, logbook_entry_id, submitted_by_user_id, submitted_at, status, applied_by_user_id, applied_at)
             VALUES (?1, ?2, ?3, ?4, 'pending', NULL, NULL)",
            params![proposal_id, entry_id, submitted_by_user_id, submitted_at],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn inventory_proposal_item_insert(
        &self,
        id: &str,
        proposal_id: &str,
        action: &str,
        division: &str,
        item_name: &str,
        category: &str,
        quantity: i64,
        confidence: Option<f64>,
        line_location: Option<&str>,
    ) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let loc: Option<String> = line_location
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        conn.execute(
            "INSERT INTO inventory_proposal_items (id, proposal_id, action, division, item_name, category, quantity, confidence, line_location)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                id,
                proposal_id,
                action,
                division.trim(),
                item_name.trim(),
                category.trim(),
                quantity,
                confidence,
                loc
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn inventory_proposal_set_status(
        &self,
        proposal_id: &str,
        status: &str,
        applied_by_user_id: Option<&str>,
        applied_at: Option<i64>,
    ) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE inventory_proposals SET status = ?2, applied_by_user_id = ?3, applied_at = ?4 WHERE id = ?1",
            params![proposal_id, status, applied_by_user_id, applied_at],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn inventory_proposal_get_status(
        &self,
        proposal_id: &str,
    ) -> Result<Option<String>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let v: Option<String> = conn
            .query_row(
                "SELECT status FROM inventory_proposals WHERE id = ?1 LIMIT 1",
                params![proposal_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        Ok(v)
    }

    pub fn inventory_proposal_list_items(
        &self,
        proposal_id: &str,
    ) -> Result<Vec<InventoryProposalItemRow>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, action, division, item_name, category, quantity, confidence, line_location
                 FROM inventory_proposal_items
                 WHERE proposal_id = ?1
                 ORDER BY rowid ASC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![proposal_id], |row| {
                Ok(InventoryProposalItemRow {
                    id: row.get(0)?,
                    action: row.get(1)?,
                    division: row.get(2)?,
                    item_name: row.get(3)?,
                    category: row.get(4)?,
                    quantity: row.get(5)?,
                    confidence: row.get(6)?,
                    line_location: row.get(7)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn list_logistics_library_entries(&self, limit: usize) -> Result<Vec<LogisticsLibraryRow>, String> {
        let cap = limit.clamp(1, 500);
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT
                   e.id,
                   e.body,
                   e.created_at,
                   e.author_name,
                   e.author_rank_level,
                   e.author_division,
                   e.author_directorate,
                   e.author_tier,
                   e.author_job_title,
                   a.section,
                   p.id AS proposal_id,
                   p.status AS proposal_status,
                   e.map_lat,
                   e.map_lon,
                   e.saved_place_id,
                   e.location_text
                 FROM logbook_entries e
                 JOIN logistics_annotations a ON a.logbook_entry_id = e.id
                 LEFT JOIN inventory_proposals p ON p.logbook_entry_id = e.id
                 ORDER BY e.created_at DESC
                 LIMIT ?1",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![cap as i64], |row| {
                Ok(LogisticsLibraryRow {
                    entry_id: row.get(0)?,
                    body: row.get(1)?,
                    created_at: row.get(2)?,
                    author_name: row.get(3).unwrap_or_default(),
                    author_rank_level: row.get(4)?,
                    author_division: row.get(5)?,
                    author_directorate: row.get(6)?,
                    author_tier: row.get(7)?,
                    author_job_title: row.get(8)?,
                    section: row.get(9).unwrap_or_else(|_| "General".to_string()),
                    proposal_id: row.get(10)?,
                    proposal_status: row.get(11)?,
                    map_lat: row.get(12)?,
                    map_lon: row.get(13)?,
                    saved_place_id: row.get(14)?,
                    location_text: row.get(15)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }


    /// Returns (file_size, last_modified) for a stable `relative_path` key when present.

    pub fn get_file_metadata(&self, relative_path: &str) -> Result<Option<(i64, Option<i64>)>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT size, modified_at FROM indexed_files WHERE relative_path = ?1 LIMIT 1",
            )
            .map_err(|e| e.to_string())?;
        let row = stmt.query_row(params![relative_path], |row| {
            Ok((row.get::<_, Option<i64>>(0)?, row.get::<_, Option<i64>>(1)?))
        });
        match row {
            Ok((Some(sz), m)) => Ok(Some((sz, m))),
            Ok((None, _)) => Ok(None),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    /// Same as [`get_file_metadata`], but matches the indexed document `path` (absolute path as stored).
    /// Used to skip re-ingestion when `relative_path` was never set (legacy rows) or keys disagree.
    pub fn get_file_metadata_by_path(&self, indexed_path: &str) -> Result<Option<(i64, Option<i64>)>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT size, modified_at FROM indexed_files WHERE path = ?1 LIMIT 1",
            )
            .map_err(|e| e.to_string())?;
        let row = stmt.query_row(params![indexed_path], |row| {
            Ok((row.get::<_, Option<i64>>(0)?, row.get::<_, Option<i64>>(1)?))
        });
        match row {
            Ok((Some(sz), m)) => Ok(Some((sz, m))),
            Ok((None, _)) => Ok(None),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn remove_file_from_index(&self, path: &str) -> Result<(), String> {
        self.delete_embeddings_for_file(path)?;
        self.delete_by_path(path)?;
        Ok(())
    }

    /// Paths in the library index that live under `root` (files, subfolders, or ZIM articles for a ZIM at root).
    pub fn list_indexed_paths_for_root(&self, root: &str) -> Result<Vec<String>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let r = root.trim_end_matches(&['/', '\\'][..]);
        let like_slash = format!("{}/%", r);
        let like_back = format!("{}\\%", r);
        let like_zim = format!("{}::%", r);
        let mut stmt = conn
            .prepare(
                "SELECT path FROM indexed_files WHERE path = ?1 OR path LIKE ?2 OR path LIKE ?3 OR path LIKE ?4",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(
                params![r, like_slash, like_back, like_zim],
                |row| row.get::<_, String>(0),
            )
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    /// Lightweight rows for LAN mesh `/mesh/manifest` (path + type + size); capped for P2P payloads.
    pub fn list_mesh_manifest_rows(&self, limit: usize) -> Result<Vec<MeshManifestRow>, String> {
        let cap = limit.clamp(1, 100_000);
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT path, file_type, COALESCE(size, 0) AS sz, modified_at FROM indexed_files ORDER BY path LIMIT ?1",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([cap as i64], |row| {
                Ok(MeshManifestRow {
                    path: row.get(0)?,
                    file_type: row.get(1)?,
                    size_bytes: row.get::<_, i64>(2)? as u64,
                    modified_at: row.get(3)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    /// Returns on-disk size for an indexed path (`path` or `relative_path` match).
    pub fn indexed_file_size_by_path(&self, path: &str) -> Result<Option<u64>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT COALESCE(size, 0) FROM indexed_files WHERE path = ?1 OR relative_path = ?1 LIMIT 1",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query_map(params![path], |row| row.get::<_, i64>(0))
            .map_err(|e| e.to_string())?;
        if let Some(r) = rows.next() {
            let sz = r.map_err(|e| e.to_string())? as u64;
            return Ok(Some(sz));
        }
        Ok(None)
    }

    /// `indexed_files.modified_at` for a path (`path` or `relative_path` match).
    pub fn indexed_file_mtime_by_path(&self, path: &str) -> Result<Option<i64>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT modified_at FROM indexed_files WHERE path = ?1 OR relative_path = ?1 LIMIT 1",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query_map(params![path], |row| row.get::<_, Option<i64>>(0))
            .map_err(|e| e.to_string())?;
        if let Some(r) = rows.next() {
            return Ok(r.map_err(|e| e.to_string())?);
        }
        Ok(None)
    }

    /// Minimal indexed file fields needed to backfill embeddings / RAG rows without re-reading the file.
    pub fn indexed_file_rag_backfill_seed(
        &self,
        path_or_relative_path: &str,
    ) -> Result<Option<(String, String, String)>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT file_type, COALESCE(display_title, name, path) AS title, COALESCE(text_content, '') AS text
                 FROM indexed_files
                 WHERE path = ?1 OR relative_path = ?1
                 LIMIT 1",
            )
            .map_err(|e| e.to_string())?;
        let row = stmt.query_row(params![path_or_relative_path], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        });
        match row {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    /// Count of embedding rows for a library file (used to detect missing vector coverage).
    pub fn chunk_embedding_count_for_file_path(&self, file_path: &str) -> Result<i64, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM chunk_embeddings WHERE file_path = ?1",
                params![file_path],
                |row| row.get(0),
            )
            .unwrap_or(0);
        Ok(n)
    }

    /// Upsert a `rag_documents` summary row for a file already represented in `chunk_embeddings`.
    /// This makes new/updated watched-folder files immediately visible to the RAG UI and eligible for retrieval.
    pub fn upsert_rag_document_for_file_path(
        &self,
        file_path: &str,
        file_type: &str,
    ) -> Result<(), String> {
        let fp = file_path.trim();
        if fp.is_empty() {
            return Ok(());
        }
        let filename = std::path::Path::new(fp)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(fp)
            .to_string();
        let total_chunks: i64 = {
            let conn = self.conn.lock().map_err(|e| e.to_string())?;
            conn.query_row(
                "SELECT COUNT(*) FROM chunk_embeddings WHERE file_path = ?1 AND IFNULL(chunk_layer, '') != 'parent'",
                params![fp],
                |row| row.get(0),
            )
            .unwrap_or(0)
        };
        if total_chunks <= 0 {
            return Ok(());
        }
        let file_type = if file_type.trim().is_empty() {
            std::path::Path::new(fp)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("file")
        } else {
            file_type.trim()
        };
        let ingested_at = Utc::now().to_rfc3339();
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO rag_documents (id, filename, file_type, total_chunks, ingested_at, source_package)
             VALUES (?1, ?2, ?3, ?4, ?5, COALESCE((SELECT source_package FROM rag_documents WHERE id = ?1), 'Downloads'))
             ON CONFLICT(id) DO UPDATE SET
               filename = excluded.filename,
               file_type = excluded.file_type,
               total_chunks = excluded.total_chunks,
               ingested_at = excluded.ingested_at",
            params![fp, filename, file_type, total_chunks, ingested_at],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Update (or backfill) the `rag_documents.source_package` label for a file.
    /// Called by the background ingestion worker after embedding completion is queued.
    pub fn upsert_rag_document_source_package(
        &self,
        file_path: &str,
        source_package: &str,
    ) -> Result<(), String> {
        let fp = file_path.trim();
        let src = source_package.trim();
        if fp.is_empty() || src.is_empty() {
            return Ok(());
        }
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE rag_documents SET source_package = ?2 WHERE id = ?1",
            params![fp, src],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn upsert_file(
        &self,
        path: &str,
        file_type: &str,
        name: &str,
        size: Option<i64>,
        modified_at: Option<i64>,
        text_content: Option<&str>,
        metadata_json: Option<&str>,
        display_title: Option<&str>,
        page_count: Option<i64>,
        relative_path: Option<&str>,
    ) -> Result<(), String> {
        let now = Utc::now().timestamp();
        with_sql_connection_busy_retries(&self.conn, |conn| {
            conn.execute(
                "
            INSERT INTO indexed_files (path, file_type, name, size, modified_at, text_content, metadata_json, created_at, display_title, page_count, relative_path)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            ON CONFLICT(path) DO UPDATE SET
                file_type = excluded.file_type,
                name = excluded.name,
                size = excluded.size,
                modified_at = excluded.modified_at,
                text_content = excluded.text_content,
                metadata_json = excluded.metadata_json,
                display_title = excluded.display_title,
                page_count = excluded.page_count,
                relative_path = excluded.relative_path
            ",
                params![
                    path,
                    file_type,
                    name,
                    size,
                    modified_at,
                    text_content,
                    metadata_json,
                    now,
                    display_title,
                    page_count,
                    relative_path
                ],
            )
        })?;
        Ok(())
    }

    pub fn delete_by_path(&self, path: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let dir_prefix = format!("{}/", path.trim_end_matches('/'));
        let derived_prefix = format!("{}::", path);
        conn.execute(
            "DELETE FROM indexed_files WHERE path = ?1 OR path LIKE ?2 OR path LIKE ?3",
            params![
                path,
                format!("{}%", dir_prefix),
                format!("{}%", derived_prefix)
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn get_by_path(&self, path: &str) -> Result<Option<IndexedFile>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, path, file_type, name, size, modified_at, text_content, metadata_json, created_at, display_title, page_count
                 FROM indexed_files WHERE path = ?1 LIMIT 1",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query(params![path]).map_err(|e| e.to_string())?;
        if let Some(row) = rows.next().map_err(|e| e.to_string())? {
            Ok(Some(IndexedFile {
                id: row.get(0).map_err(|e| e.to_string())?,
                path: row.get(1).map_err(|e| e.to_string())?,
                file_type: row.get(2).map_err(|e| e.to_string())?,
                name: row.get(3).map_err(|e| e.to_string())?,
                size: row.get(4).map_err(|e| e.to_string())?,
                modified_at: row.get(5).map_err(|e| e.to_string())?,
                text_content: row.get(6).map_err(|e| e.to_string())?,
                metadata_json: row.get(7).map_err(|e| e.to_string())?,
                created_at: row.get(8).map_err(|e| e.to_string())?,
                display_title: row.get(9).map_err(|e| e.to_string())?,
                page_count: row.get(10).map_err(|e| e.to_string())?,
            }))
        } else {
            Ok(None)
        }
    }

    /// Deletes all indexed entries whose path starts with `prefix` (e.g. a ZIM file path, so "file.zim::" sub-entries are removed).
    pub fn delete_by_path_prefix(&self, prefix: &str) -> Result<(), String> {
        let like = format!("{}%", prefix);
        with_sql_connection_busy_retries(&self.conn, |conn| {
            conn.execute(
                "DELETE FROM indexed_files WHERE path = ?1 OR path LIKE ?2",
                params![prefix, like],
            )
        })?;
        Ok(())
    }

    /// If `archive_watch_roots` is empty, seed one row from legacy `watch_folder` in config.
    pub fn migrate_archive_watch_roots_from_config(&self, app_data_dir: &Path) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM archive_watch_roots", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        if count > 0 {
            return Ok(());
        }
        let cfg = load_config(app_data_dir);
        if let Some(ref wf) = cfg.watch_folder {
            let p = wf.trim();
            if !p.is_empty() {
                let now = Utc::now().timestamp();
                let _ = conn.execute(
                    "INSERT INTO archive_watch_roots (path, label, sort_index, created_at) VALUES (?1, NULL, 0, ?2)",
                    params![p, now],
                );
            }
        }
        Ok(())
    }

    pub fn list_archive_watch_roots(&self) -> Result<Vec<ArchiveWatchRootRow>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, path, label, sort_index FROM archive_watch_roots ORDER BY sort_index ASC, id ASC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok(ArchiveWatchRootRow {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    label: row.get(2)?,
                    sort_index: row.get(3)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn list_archive_watch_root_paths_ordered(&self) -> Result<Vec<String>, String> {
        Ok(self
            .list_archive_watch_roots()?
            .into_iter()
            .map(|r| r.path)
            .collect())
    }

    pub fn clear_archive_watch_roots(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM archive_watch_roots", [])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn add_archive_watch_root(
        &self,
        path: &str,
        label: Option<&str>,
    ) -> Result<i64, String> {
        let p = path.trim();
        if p.is_empty() {
            return Err("Path is empty.".to_string());
        }
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let max_sort: i32 = conn
            .query_row(
                "SELECT COALESCE(MAX(sort_index), -1) + 1 FROM archive_watch_roots",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let now = Utc::now().timestamp();
        conn.execute(
            "INSERT INTO archive_watch_roots (path, label, sort_index, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![p, label, max_sort, now],
        )
        .map_err(|e| e.to_string())?;
        Ok(conn.last_insert_rowid())
    }

    pub fn remove_archive_watch_root(&self, id: i64) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let n = conn
            .execute("DELETE FROM archive_watch_roots WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("No data source with that id.".to_string());
        }
        Ok(())
    }

    pub fn first_archive_watch_root_path(&self) -> Result<Option<String>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare("SELECT path FROM archive_watch_roots ORDER BY sort_index ASC, id ASC LIMIT 1")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
        if let Some(row) = rows.next().map_err(|e| e.to_string())? {
            Ok(Some(row.get::<_, String>(0).map_err(|e| e.to_string())?))
        } else {
            Ok(None)
        }
    }

    /// The `path` value from `archive_watch_roots` whose resolved location matches `resolved_root`.
    pub fn archive_watch_root_stored_path_for_resolved(
        &self,
        resolved_root: &std::path::Path,
    ) -> Result<Option<String>, String> {
        let target = resolved_root.canonicalize().map_err(|e| e.to_string())?;
        for row in self.list_archive_watch_roots()? {
            let p = std::path::Path::new(row.path.trim());
            if let Ok(c) = p.canonicalize() {
                if c == target {
                    return Ok(Some(row.path));
                }
            }
        }
        Ok(None)
    }

    /// Indexed file paths under any of `root_prefixes` (string prefix match on stored paths).
    pub fn list_indexed_paths_under_roots(
        &self,
        root_prefixes: &[String],
    ) -> Result<Vec<(String, String)>, String> {
        if root_prefixes.is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare("SELECT path, name FROM indexed_files")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            let (path, name) = r.map_err(|e| e.to_string())?;
            let norm = path.trim();
            if root_prefixes.iter().any(|root| {
                let r = root.trim_end_matches(['/', '\\']);
                norm == r
                    || norm.starts_with(&format!("{}/", r))
                    || norm.starts_with(&format!("{}\\", r))
            }) {
                out.push((path, name));
            }
        }
        Ok(out)
    }

    pub fn get_all(&self) -> Result<Vec<IndexedFile>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare("SELECT id, path, file_type, name, size, modified_at, text_content, metadata_json, created_at, display_title, page_count FROM indexed_files ORDER BY path")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok(IndexedFile {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    file_type: row.get(2)?,
                    name: row.get(3)?,
                    size: row.get(4)?,
                    modified_at: row.get(5)?,
                    text_content: row.get(6)?,
                    metadata_json: row.get(7)?,
                    created_at: row.get(8)?,
                    display_title: row.get(9)?,
                    page_count: row.get(10)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn search_text_content(&self, query: &str, limit: i64) -> Result<Vec<IndexedFile>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut out = Vec::new();

        let keywords = extract_keywords(query, 20);
        let fts_ready = Self::table_exists(&conn, "library_chunks_fts").unwrap_or(false);

        if fts_ready && !keywords.is_empty() {
            let match_q = fts_keywords_to_match_query(&keywords);
            if let Some(mq) = match_q {
                let cap = limit.clamp(1, 50);
                let sql = r#"
SELECT f.id, f.path, f.file_type, f.name, f.size, f.modified_at,
       TRIM(COALESCE(c.retrieval_text, c.chunk_text, '')) AS snippet,
       f.metadata_json, f.created_at, f.display_title, f.page_count,
       library_chunks_fts.rank AS bm25_score
FROM library_chunks_fts
INNER JOIN chunk_embeddings c ON c.id = library_chunks_fts.rowid
LEFT JOIN indexed_files f ON f.path = c.file_path
WHERE library_chunks_fts MATCH ?1
  AND IFNULL(c.chunk_layer, '') != 'parent'
  AND (f.path IS NULL OR f.file_type = 'text' OR f.file_type = 'zim' OR f.file_type = 'learned' OR f.file_type = 'image')
ORDER BY bm25_score ASC
LIMIT ?2
"#;
                match conn.prepare(sql) {
                    Ok(mut stmt) => {
                        let rows = stmt
                            .query_map(params![mq.as_str(), cap], |row| {
                                let snippet: String = row.get(6)?;
                                Ok(IndexedFile {
                                    id: row.get(0)?,
                                    path: row.get(1)?,
                                    file_type: row.get(2)?,
                                    name: row.get(3)?,
                                    size: row.get(4)?,
                                    modified_at: row.get(5)?,
                                    text_content: Some(snippet),
                                    metadata_json: row.get(7)?,
                                    created_at: row.get(8)?,
                                    display_title: row.get(9)?,
                                    page_count: row.get(10)?,
                                })
                            })
                            .map_err(|e| e.to_string())?;
                        for row in rows {
                            out.push(row.map_err(|e| e.to_string())?);
                        }
                        if !out.is_empty() {
                            return Ok(out);
                        }
                    }
                    Err(e) => {
                        eprintln!("[athena] library_chunks_fts search prepare failed: {e}");
                    }
                }
            }
        }

        // Fallback: return recent text rows (helps when query is empty or all stopwords).
        let mut stmt = conn
            .prepare(
                "SELECT id, path, file_type, name, size, modified_at, text_content, metadata_json, created_at, display_title, page_count
                 FROM indexed_files WHERE (file_type = 'text' OR file_type = 'zim' OR file_type = 'learned' OR file_type = 'image') AND text_content IS NOT NULL AND text_content != ''
                 ORDER BY created_at DESC LIMIT ?1",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![limit], |row| {
                Ok(IndexedFile {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    file_type: row.get(2)?,
                    name: row.get(3)?,
                    size: row.get(4)?,
                    modified_at: row.get(5)?,
                    text_content: row.get(6)?,
                    metadata_json: row.get(7)?,
                    created_at: row.get(8)?,
                    display_title: row.get(9)?,
                    page_count: row.get(10)?,
                })
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            out.push(row.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    /// Return indexed files whose name is contained in the given query (case-insensitive).
    /// Used when the user says e.g. "look in House Color.pages" to force-include that document.
    pub fn get_indexed_files_by_name_mentioned(
        &self,
        query: &str,
    ) -> Result<Vec<IndexedFile>, String> {
        let all = self.get_all()?;
        let query_lower = query.to_lowercase();
        let out: Vec<IndexedFile> = all
            .into_iter()
            .filter(|f| {
                f.text_content
                    .as_deref()
                    .is_some_and(|t| !t.trim().is_empty())
                    && query_lower.contains(&f.name.to_lowercase())
            })
            .collect();
        Ok(out)
    }

    /// Return most recently indexed files that have text (for fallback when keyword/semantic return nothing).
    pub fn get_recent_text_files(&self, limit: i64) -> Result<Vec<IndexedFile>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, path, file_type, name, size, modified_at, text_content, metadata_json, created_at, display_title, page_count
                 FROM indexed_files
                 WHERE (file_type = 'text' OR file_type = 'learned' OR file_type = 'zim' OR file_type = 'image')
                   AND text_content IS NOT NULL AND text_content != ''
                 ORDER BY created_at DESC LIMIT ?1",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![limit], |row| {
                Ok(IndexedFile {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    file_type: row.get(2)?,
                    name: row.get(3)?,
                    size: row.get(4)?,
                    modified_at: row.get(5)?,
                    text_content: row.get(6)?,
                    metadata_json: row.get(7)?,
                    created_at: row.get(8)?,
                    display_title: row.get(9)?,
                    page_count: row.get(10)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn get_stats(&self) -> Result<IndexStats, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let total: i64 = conn
            .query_row("SELECT COUNT(*) FROM indexed_files", [], |row| row.get(0))
            .map_err(|e| e.to_string())?;
        let text_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM indexed_files WHERE file_type = 'text'",
                [],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        let image_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM indexed_files WHERE file_type = 'image'",
                [],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        let video_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM indexed_files WHERE file_type = 'video'",
                [],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        let map_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM indexed_files WHERE file_type = 'map'",
                [],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        let zim_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM indexed_files WHERE file_type = 'zim'",
                [],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        Ok(IndexStats {
            total_files: total,
            text_count,
            image_count,
            video_count,
            map_count,
            zim_count,
        })
    }

    pub fn get_rag_counts(&self) -> Result<RagCounts, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let document_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM indexed_files WHERE text_content IS NOT NULL AND text_content != ''",
                [],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        let chunk_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM chunk_embeddings", [], |row| {
                row.get(0)
            })
            .map_err(|e| e.to_string())?;
        Ok(RagCounts {
            document_count,
            chunk_count,
        })
    }

    pub fn upsert_tactical_point_of_interest(
        &self,
        point: &TacticalPointOfInterest,
    ) -> Result<(), String> {
        let id = point.id.trim();
        let name = point.name.trim();
        if id.is_empty() {
            return Err("Point of interest id is required.".to_string());
        }
        if name.is_empty() {
            return Err("Point of interest name is required.".to_string());
        }
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO points_of_interest (id, name, lat, lon, notes, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
               name = excluded.name,
               lat = excluded.lat,
               lon = excluded.lon,
               notes = excluded.notes,
               updated_at = excluded.updated_at",
            params![
                id,
                name,
                point.lat,
                point.lon,
                point.notes,
                point.updated_at
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn upsert_tactical_mesh_node_location(
        &self,
        node: &TacticalMeshNodeLocation,
    ) -> Result<(), String> {
        let node_id = node.node_id.trim();
        if node_id.is_empty() {
            return Err("Mesh node id is required.".to_string());
        }
        let label = node.label.trim();
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO mesh_node_locations (node_id, label, lat, lon, last_seen_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(node_id) DO UPDATE SET
               label = excluded.label,
               lat = excluded.lat,
               lon = excluded.lon,
               last_seen_at = excluded.last_seen_at",
            params![node_id, label, node.lat, node.lon, node.last_seen_at],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn delete_tactical_point_of_interest(&self, id: &str) -> Result<(), String> {
        let id = id.trim();
        if id.is_empty() {
            return Err("Point of interest id is required.".to_string());
        }
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let n = conn
            .execute("DELETE FROM points_of_interest WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("Tactical point not found.".to_string());
        }
        Ok(())
    }

    pub fn get_tactical_point_of_interest_by_id(
        &self,
        id: &str,
    ) -> Result<Option<TacticalPointOfInterest>, String> {
        let id = id.trim();
        if id.is_empty() {
            return Err("Point of interest id is required.".to_string());
        }
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut statement = conn
            .prepare(
                "SELECT id, name, lat, lon, notes, updated_at
                 FROM points_of_interest
                 WHERE id = ?1",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = statement
            .query_map(params![id], |row| {
                Ok(TacticalPointOfInterest {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    lat: row.get(2)?,
                    lon: row.get(3)?,
                    notes: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.next().transpose().map_err(|e| e.to_string())
    }

    pub fn list_tactical_points_of_interest(&self) -> Result<Vec<TacticalPointOfInterest>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut statement = conn
            .prepare(
                "SELECT id, name, lat, lon, notes, updated_at
                 FROM points_of_interest
                 ORDER BY name COLLATE NOCASE, id",
            )
            .map_err(|e| e.to_string())?;
        let rows = statement
            .query_map([], |row| {
                Ok(TacticalPointOfInterest {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    lat: row.get(2)?,
                    lon: row.get(3)?,
                    notes: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn list_tactical_mesh_node_locations(
        &self,
    ) -> Result<Vec<TacticalMeshNodeLocation>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut statement = conn
            .prepare(
                "SELECT node_id, label, lat, lon, last_seen_at
                 FROM mesh_node_locations
                 ORDER BY node_id",
            )
            .map_err(|e| e.to_string())?;
        let rows = statement
            .query_map([], |row| {
                Ok(TacticalMeshNodeLocation {
                    node_id: row.get(0)?,
                    label: row.get(1)?,
                    lat: row.get(2)?,
                    lon: row.get(3)?,
                    last_seen_at: row.get(4)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn list_map_markers(&self) -> Result<Vec<MapMarker>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut statement = conn
            .prepare(
                "SELECT id, lat, lng, title, description, created_at, category
                 FROM map_markers
                 ORDER BY created_at DESC",
            )
            .map_err(|e| e.to_string())?;
        let rows = statement
            .query_map([], |row| {
                Ok(MapMarker {
                    id: row.get(0)?,
                    lat: row.get(1)?,
                    lng: row.get(2)?,
                    title: row.get(3)?,
                    description: row.get(4)?,
                    created_at: row.get(5)?,
                    category: row.get(6)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn get_map_marker_created_at(&self, id: &str) -> Result<Option<String>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare("SELECT created_at FROM map_markers WHERE id = ?1 LIMIT 1")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query(params![id.trim()]).map_err(|e| e.to_string())?;
        match rows.next() {
            Ok(Some(row)) => Ok(Some(row.get::<_, String>(0).map_err(|e| e.to_string())?)),
            Ok(None) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn list_map_markers_since_created_at(
        &self,
        since_created_at: &str,
        limit: usize,
    ) -> Result<Vec<MapMarker>, String> {
        let cap = limit.clamp(1, 500);
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, lat, lng, title, description, created_at, category
                 FROM map_markers
                 WHERE created_at > ?1
                 ORDER BY created_at DESC
                 LIMIT ?2",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![since_created_at, cap as i64], |row| {
                Ok(MapMarker {
                    id: row.get(0)?,
                    lat: row.get(1)?,
                    lng: row.get(2)?,
                    title: row.get(3)?,
                    description: row.get(4)?,
                    created_at: row.get(5)?,
                    category: row.get(6)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn get_max_map_marker_created_at(&self) -> Result<Option<String>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.query_row("SELECT MAX(created_at) FROM map_markers", [], |row| {
            row.get::<_, Option<String>>(0)
        })
        .map_err(|e| e.to_string())
    }

    pub fn insert_map_marker(&self, marker: &MapMarker) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO map_markers (id, lat, lng, title, description, created_at, category)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                marker.id,
                marker.lat,
                marker.lng,
                marker.title,
                marker.description,
                marker.created_at,
                marker.category
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn upsert_map_marker(&self, marker: &MapMarker) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO map_markers (id, lat, lng, title, description, created_at, category)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
               lat = excluded.lat,
               lng = excluded.lng,
               title = excluded.title,
               description = excluded.description,
               created_at = excluded.created_at,
               category = excluded.category",
            params![
                marker.id,
                marker.lat,
                marker.lng,
                marker.title,
                marker.description,
                marker.created_at,
                marker.category
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// LAN merge: never overwrite a newer local marker (based on `created_at` monotonicity).
    /// Returns `true` if a row was inserted/updated, `false` if the incoming row was older or identical.
    pub fn upsert_map_marker_if_newer(&self, marker: &MapMarker) -> Result<bool, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let n = conn
            .execute(
                "INSERT INTO map_markers (id, lat, lng, title, description, created_at, category)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
               lat = excluded.lat,
               lng = excluded.lng,
               title = excluded.title,
               description = excluded.description,
               created_at = excluded.created_at,
               category = excluded.category
             WHERE excluded.created_at > map_markers.created_at",
                params![
                    marker.id,
                    marker.lat,
                    marker.lng,
                    marker.title,
                    marker.description,
                    marker.created_at,
                    marker.category
                ],
            )
            .map_err(|e| e.to_string())?;
        Ok(n > 0)
    }

    pub fn get_map_marker_by_id(&self, id: &str) -> Result<Option<MapMarker>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, lat, lng, title, description, created_at, category
                 FROM map_markers
                 WHERE id = ?1
                 LIMIT 1",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query(params![id.trim()]).map_err(|e| e.to_string())?;
        if let Some(row) = rows.next().map_err(|e| e.to_string())? {
            Ok(Some(MapMarker {
                id: row.get(0).map_err(|e| e.to_string())?,
                lat: row.get(1).map_err(|e| e.to_string())?,
                lng: row.get(2).map_err(|e| e.to_string())?,
                title: row.get(3).map_err(|e| e.to_string())?,
                description: row.get(4).map_err(|e| e.to_string())?,
                created_at: row.get(5).map_err(|e| e.to_string())?,
                category: row.get(6).map_err(|e| e.to_string())?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn delete_map_marker(&self, id: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM map_markers WHERE id = ?1", params![id.trim()])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn insert_map_drawing(&self, drawing: &MapDrawing) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO map_drawings (id, feature_type, geometry_json, style_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                drawing.id,
                drawing.feature_type,
                drawing.geometry_json,
                drawing.style_json,
                drawing.created_at
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn list_map_drawings(&self) -> Result<Vec<MapDrawing>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut statement = conn
            .prepare(
                "SELECT id, feature_type, geometry_json, style_json, created_at
                 FROM map_drawings
                 ORDER BY created_at ASC",
            )
            .map_err(|e| e.to_string())?;
        let rows = statement
            .query_map([], |row| {
                Ok(MapDrawing {
                    id: row.get(0)?,
                    feature_type: row.get(1)?,
                    geometry_json: row.get(2)?,
                    style_json: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn delete_map_drawing(&self, id: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM map_drawings WHERE id = ?1", params![id.trim()])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn insert_radio_log(&self, log: &RadioLog) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO radio_logs (id, direction, message, timestamp) VALUES (?1, ?2, ?3, ?4)",
            params![log.id, log.direction, log.message, log.timestamp],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn list_recent_radio_logs(&self, limit: usize) -> Result<Vec<RadioLog>, String> {
        let cap = limit.clamp(1, 500);
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut statement = conn
            .prepare(
                "SELECT id, direction, message, timestamp
                 FROM (
                    SELECT id, direction, message, timestamp
                    FROM radio_logs
                    ORDER BY timestamp DESC
                    LIMIT ?1
                 )
                 ORDER BY timestamp ASC",
            )
            .map_err(|e| e.to_string())?;
        let rows = statement
            .query_map(params![cap as i64], |row| {
                Ok(RadioLog {
                    id: row.get(0)?,
                    direction: row.get(1)?,
                    message: row.get(2)?,
                    timestamp: row.get(3)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    /// All radio log rows in chronological order (oldest first), for CSV export.
    pub fn list_all_radio_logs(&self) -> Result<Vec<RadioLog>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut statement = conn
            .prepare(
                "SELECT id, direction, message, timestamp
                 FROM radio_logs
                 ORDER BY timestamp ASC",
            )
            .map_err(|e| e.to_string())?;
        let rows = statement
            .query_map([], |row| {
                Ok(RadioLog {
                    id: row.get(0)?,
                    direction: row.get(1)?,
                    message: row.get(2)?,
                    timestamp: row.get(3)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    /// Returns `Some(sample)` when a new row was inserted (dedupe by timestamp + source).
    pub fn insert_radio_telemetry(
        &self,
        source_node_id: u32,
        rssi: i64,
        snr: f64,
    ) -> Result<Option<RadioTelemetrySample>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let ts = Utc::now().to_rfc3339();
        let n = conn
            .execute(
                "INSERT OR IGNORE INTO radio_telemetry (timestamp, source_node_id, rssi, snr) VALUES (?1, ?2, ?3, ?4)",
                params![ts, source_node_id as i64, rssi, snr],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Ok(None);
        }
        Ok(Some(RadioTelemetrySample {
            id: conn.last_insert_rowid(),
            timestamp: ts,
            source_node_id,
            rssi,
            snr,
        }))
    }

    pub fn list_recent_radio_telemetry(
        &self,
        limit: usize,
    ) -> Result<Vec<RadioTelemetrySample>, String> {
        let cap = limit.clamp(1, 2000);
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut statement = conn
            .prepare(
                "SELECT id, timestamp, source_node_id, rssi, snr
                 FROM radio_telemetry
                 ORDER BY timestamp DESC
                 LIMIT ?1",
            )
            .map_err(|e| e.to_string())?;
        let rows = statement
            .query_map(params![cap as i64], |row| {
                Ok(RadioTelemetrySample {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    source_node_id: row.get::<_, i64>(2)? as u32,
                    rssi: row.get(3)?,
                    snr: row.get(4)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn get_max_radio_telemetry_timestamp(&self) -> Result<Option<String>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.query_row("SELECT MAX(timestamp) FROM radio_telemetry", [], |row| {
            row.get::<_, Option<String>>(0)
        })
        .map_err(|e| e.to_string())
    }

    pub fn list_radio_telemetry_since(
        &self,
        since_timestamp: &str,
        limit: usize,
    ) -> Result<Vec<RadioTelemetrySample>, String> {
        let cap = limit.clamp(1, 1000);
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, timestamp, source_node_id, rssi, snr
                 FROM radio_telemetry
                 WHERE timestamp > ?1
                 ORDER BY timestamp DESC
                 LIMIT ?2",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![since_timestamp, cap as i64], |row| {
                Ok(RadioTelemetrySample {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    source_node_id: row.get::<_, i64>(2)? as u32,
                    rssi: row.get(3)?,
                    snr: row.get(4)?,
                })
            })
            .map_err(|e| e.to_string())?;

        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn insert_radio_telemetry_from_lan(
        &self,
        source_node_id: u32,
        timestamp: &str,
        rssi: i64,
        snr: f64,
    ) -> Result<bool, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let n = conn
            .execute(
                "INSERT OR IGNORE INTO radio_telemetry (timestamp, source_node_id, rssi, snr)
                 VALUES (?1, ?2, ?3, ?4)",
                params![timestamp, source_node_id as i64, rssi, snr],
            )
            .map_err(|e| e.to_string())?;
        Ok(n > 0)
    }

    pub fn insert_mesh_message_pending(
        &self,
        packet_id: &str,
        message: &str,
    ) -> Result<MeshMessage, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO mesh_messages (packet_id, message, status, created_at, updated_at)
             VALUES (?1, ?2, 'pending', ?3, ?3)",
            params![packet_id, message, now],
        )
        .map_err(|e| e.to_string())?;
        Ok(MeshMessage {
            packet_id: packet_id.to_string(),
            message: message.to_string(),
            status: "pending".to_string(),
            created_at: now.clone(),
            updated_at: now,
        })
    }

    pub fn mark_mesh_message_delivered(
        &self,
        packet_id: &str,
    ) -> Result<Option<MeshMessage>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let now = Utc::now().to_rfc3339();
        let n = conn
            .execute(
                "UPDATE mesh_messages SET status = 'delivered', updated_at = ?2 WHERE packet_id = ?1 AND status = 'pending'",
                params![packet_id.trim(), now],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Ok(None);
        }
        conn.query_row(
            "SELECT packet_id, message, status, created_at, updated_at FROM mesh_messages WHERE packet_id = ?1 LIMIT 1",
            params![packet_id.trim()],
            |row| {
                Ok(MeshMessage {
                    packet_id: row.get(0)?,
                    message: row.get(1)?,
                    status: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                })
            },
        )
        .map(Some)
        .map_err(|e| e.to_string())
    }

    pub fn list_recent_mesh_messages(&self, limit: usize) -> Result<Vec<MeshMessage>, String> {
        let cap = limit.clamp(1, 500);
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut statement = conn
            .prepare(
                "SELECT packet_id, message, status, created_at, updated_at
                 FROM mesh_messages
                 ORDER BY created_at DESC
                 LIMIT ?1",
            )
            .map_err(|e| e.to_string())?;
        let rows = statement
            .query_map(params![cap as i64], |row| {
                Ok(MeshMessage {
                    packet_id: row.get(0)?,
                    message: row.get(1)?,
                    status: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn list_mesh_messages_updated_since(
        &self,
        since_updated_at: &str,
        limit: usize,
    ) -> Result<Vec<MeshMessage>, String> {
        let cap = limit.clamp(1, 200);
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT packet_id, message, status, created_at, updated_at
                 FROM mesh_messages
                 WHERE updated_at > ?1
                 ORDER BY updated_at DESC
                 LIMIT ?2",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![since_updated_at, cap as i64], |row| {
                Ok(MeshMessage {
                    packet_id: row.get(0)?,
                    message: row.get(1)?,
                    status: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                })
            })
            .map_err(|e| e.to_string())?;

        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn get_max_mesh_messages_updated_at(&self) -> Result<Option<String>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.query_row("SELECT MAX(updated_at) FROM mesh_messages", [], |row| {
            row.get::<_, Option<String>>(0)
        })
        .map_err(|e| e.to_string())
    }

    /// LAN merge for mesh message status without regressing a delivered packet back to pending.
    /// Returns `true` if the DB changed.
    pub fn merge_mesh_message_from_lan(&self, msg: &MeshMessage) -> Result<bool, String> {
        let packet_id = msg.packet_id.trim();
        if packet_id.is_empty() {
            return Ok(false);
        }
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare("SELECT status, updated_at FROM mesh_messages WHERE packet_id = ?1 LIMIT 1")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query(params![packet_id]).map_err(|e| e.to_string())?;

        match rows.next() {
            Ok(Some(existing)) => {
                let existing_status: String = existing.get(0).map_err(|e| e.to_string())?;
                let existing_updated_at: String = existing.get(1).map_err(|e| e.to_string())?;

                if msg.updated_at <= existing_updated_at {
                    return Ok(false);
                }

                // Never regress delivered -> pending.
                if existing_status == "delivered" && msg.status == "pending" {
                    return Ok(false);
                }

                let n = conn
                    .execute(
                        "UPDATE mesh_messages
                         SET message = ?1, status = ?2, created_at = ?3, updated_at = ?4
                         WHERE packet_id = ?5",
                        params![
                            msg.message,
                            msg.status,
                            msg.created_at,
                            msg.updated_at,
                            packet_id
                        ],
                    )
                    .map_err(|e| e.to_string())?;
                return Ok(n > 0);
            }
            Ok(None) => {}
            Err(e) => return Err(e.to_string()),
        }

        let n = conn
            .execute(
                "INSERT INTO mesh_messages (packet_id, message, status, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(packet_id) DO NOTHING",
                params![
                    packet_id,
                    msg.message,
                    msg.status,
                    msg.created_at,
                    msg.updated_at
                ],
            )
            .map_err(|e| e.to_string())?;
        Ok(n > 0)
    }

    pub fn list_radio_channels(&self) -> Result<Vec<RadioChannel>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut statement = conn
            .prepare(
                "SELECT id, name, frequency_mhz, encryption_key, created_at
                 FROM radio_channels
                 ORDER BY name ASC",
            )
            .map_err(|e| e.to_string())?;
        let rows = statement
            .query_map([], |row| {
                Ok(RadioChannel {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    frequency_mhz: row.get(2)?,
                    encryption_key: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn upsert_radio_channel(&self, channel: &RadioChannel) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO radio_channels (id, name, frequency_mhz, encryption_key, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
               name = excluded.name,
               frequency_mhz = excluded.frequency_mhz,
               encryption_key = excluded.encryption_key",
            params![
                channel.id,
                channel.name,
                channel.frequency_mhz,
                channel.encryption_key,
                channel.created_at
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn get_radio_channel_by_id(&self, id: &str) -> Result<Option<RadioChannel>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, name, frequency_mhz, encryption_key, created_at
                 FROM radio_channels WHERE id = ?1 LIMIT 1",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query(params![id.trim()]).map_err(|e| e.to_string())?;
        if let Some(row) = rows.next().map_err(|e| e.to_string())? {
            Ok(Some(RadioChannel {
                id: row.get(0).map_err(|e| e.to_string())?,
                name: row.get(1).map_err(|e| e.to_string())?,
                frequency_mhz: row.get(2).map_err(|e| e.to_string())?,
                encryption_key: row.get(3).map_err(|e| e.to_string())?,
                created_at: row.get(4).map_err(|e| e.to_string())?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn delete_radio_channel(&self, id: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "DELETE FROM radio_channels WHERE id = ?1",
            params![id.trim()],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn import_tactical_data(
        &self,
        points_of_interest: &[TacticalPointOfInterest],
        mesh_node_locations: &[TacticalMeshNodeLocation],
    ) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;

        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO points_of_interest (id, name, lat, lon, notes, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                     ON CONFLICT(id) DO UPDATE SET
                       name = excluded.name,
                       lat = excluded.lat,
                       lon = excluded.lon,
                       notes = excluded.notes,
                       updated_at = excluded.updated_at",
                )
                .map_err(|e| e.to_string())?;
            for point in points_of_interest {
                let id = point.id.trim();
                let name = point.name.trim();
                if id.is_empty() || name.is_empty() {
                    continue;
                }
                stmt.execute(params![
                    id,
                    name,
                    point.lat,
                    point.lon,
                    point.notes,
                    point.updated_at
                ])
                .map_err(|e| e.to_string())?;
            }
        }

        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO mesh_node_locations (node_id, label, lat, lon, last_seen_at)
                     VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(node_id) DO UPDATE SET
                       label = excluded.label,
                       lat = excluded.lat,
                       lon = excluded.lon,
                       last_seen_at = excluded.last_seen_at",
                )
                .map_err(|e| e.to_string())?;
            for node in mesh_node_locations {
                let node_id = node.node_id.trim();
                if node_id.is_empty() {
                    continue;
                }
                stmt.execute(params![
                    node_id,
                    node.label.trim(),
                    node.lat,
                    node.lon,
                    node.last_seen_at
                ])
                .map_err(|e| e.to_string())?;
            }
        }

        tx.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Insert or replace one chunk row; returns SQLite `rowid` (matches `chunk_embeddings.id`).
    pub fn insert_chunk_embedding_row(
        &self,
        file_path: &str,
        chunk_index: i64,
        chunk_layer: Option<&str>,
        parent_id: Option<i64>,
        page_start: Option<i32>,
        page_end: Option<i32>,
        chunk_text: &str,
        retrieval_text: &str,
        vector: &[f32],
        needs_embedding: bool,
        sync_to_vec_index: bool,
        timestamp_start: Option<f64>,
    ) -> Result<i64, String> {
        if vector.len() != embeddings::EMBEDDING_DIM {
            return Err(format!(
                "vector length {} must be {}",
                vector.len(),
                embeddings::EMBEDDING_DIM
            ));
        }
        let now = Utc::now().timestamp();
        let needs_emb: i64 = if needs_embedding { 1 } else { 0 };
        let blob = bytemuck::cast_slice::<f32, u8>(vector).to_vec();
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "
            INSERT INTO chunk_embeddings (file_path, chunk_index, chunk_layer, parent_id, page_start, page_end, chunk_text, retrieval_text, vector, needs_embedding, created_at, timestamp_start)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ON CONFLICT(file_path, chunk_index) DO UPDATE SET
                chunk_layer = excluded.chunk_layer,
                parent_id = excluded.parent_id,
                page_start = excluded.page_start,
                page_end = excluded.page_end,
                chunk_text = excluded.chunk_text,
                retrieval_text = excluded.retrieval_text,
                vector = excluded.vector,
                needs_embedding = excluded.needs_embedding,
                created_at = excluded.created_at,
                timestamp_start = excluded.timestamp_start
            ",
            params![
                file_path,
                chunk_index,
                chunk_layer,
                parent_id,
                page_start,
                page_end,
                chunk_text,
                retrieval_text,
                blob.clone(),
                needs_emb,
                now,
                timestamp_start
            ],
        )
        .map_err(|e| e.to_string())?;
        let rowid = conn.last_insert_rowid();
        if sync_to_vec_index {
            let norm: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
            if norm > 1e-8 {
                let _ = conn.execute(
                    "INSERT OR REPLACE INTO chunk_embeddings_vec(rowid, embedding) VALUES (?1, ?2)",
                    params![rowid, blob],
                );
            }
        }
        Ok(rowid)
    }

    fn delete_embeddings_for_file_tx(tx: &Transaction<'_>, file_path: &str) -> Result<(), String> {
        let _ = tx.execute(
            "DELETE FROM chunk_embeddings_vec WHERE rowid IN (SELECT id FROM chunk_embeddings WHERE file_path = ?1)",
            params![file_path],
        );
        tx.execute(
            "DELETE FROM chunk_embeddings WHERE file_path = ?1",
            params![file_path],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn insert_chunk_embedding_row_tx(
        tx: &Transaction<'_>,
        file_path: &str,
        chunk_index: i64,
        chunk_layer: Option<&str>,
        parent_id: Option<i64>,
        page_start: Option<i32>,
        page_end: Option<i32>,
        chunk_text: &str,
        retrieval_text: &str,
        vector: &[f32],
        needs_embedding: bool,
        sync_to_vec_index: bool,
        timestamp_start: Option<f64>,
    ) -> Result<i64, String> {
        if vector.len() != embeddings::EMBEDDING_DIM {
            return Err(format!(
                "vector length {} must be {}",
                vector.len(),
                embeddings::EMBEDDING_DIM
            ));
        }
        let now = Utc::now().timestamp();
        let needs_emb: i64 = if needs_embedding { 1 } else { 0 };
        let blob = bytemuck::cast_slice::<f32, u8>(vector).to_vec();
        tx.execute(
            "
            INSERT INTO chunk_embeddings (file_path, chunk_index, chunk_layer, parent_id, page_start, page_end, chunk_text, retrieval_text, vector, needs_embedding, created_at, timestamp_start)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ON CONFLICT(file_path, chunk_index) DO UPDATE SET
                chunk_layer = excluded.chunk_layer,
                parent_id = excluded.parent_id,
                page_start = excluded.page_start,
                page_end = excluded.page_end,
                chunk_text = excluded.chunk_text,
                retrieval_text = excluded.retrieval_text,
                vector = excluded.vector,
                needs_embedding = excluded.needs_embedding,
                created_at = excluded.created_at,
                timestamp_start = excluded.timestamp_start
            ",
            params![
                file_path,
                chunk_index,
                chunk_layer,
                parent_id,
                page_start,
                page_end,
                chunk_text,
                retrieval_text,
                blob.clone(),
                needs_emb,
                now,
                timestamp_start
            ],
        )
        .map_err(|e| e.to_string())?;
        let rowid = tx.last_insert_rowid();
        if sync_to_vec_index {
            let norm: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
            if norm > 1e-8 {
                let _ = tx.execute(
                    "INSERT OR REPLACE INTO chunk_embeddings_vec(rowid, embedding) VALUES (?1, ?2)",
                    params![rowid, blob],
                );
            }
        }
        Ok(rowid)
    }

    /// Batched chunk embedding write: delete once, then insert in transactions of at most
    /// `CHUNK_EMBED_SQLITE_BATCH` rows so WAL checkpoints stay bounded on huge libraries.
    pub const CHUNK_EMBED_SQLITE_BATCH: usize = 500;

    /// Batched chunk embedding write: single transaction for delete + inserts.
    pub fn replace_file_chunk_embeddings_batched(
        &self,
        file_path: &str,
        rows: Vec<(
            i64,
            Option<&str>,
            Option<i64>,
            Option<i32>,
            Option<i32>,
            String,
            String,
            Vec<f32>,
            bool,
            bool,
            Option<f64>,
        )>,
    ) -> Result<(), String> {
        let mut conn = self.conn.lock().map_err(|e| e.to_string())?;
        {
            let tx = conn.transaction().map_err(|e| e.to_string())?;
            Self::delete_embeddings_for_file_tx(&tx, file_path)?;
            tx.commit().map_err(|e| e.to_string())?;
        }
        let mut last_parent_rowid: Option<i64> = None;
        let mut iter = rows.into_iter().peekable();
        while iter.peek().is_some() {
            let tx = conn.transaction().map_err(|e| e.to_string())?;
            let mut n_in_tx: usize = 0;
            while n_in_tx < Self::CHUNK_EMBED_SQLITE_BATCH {
                let Some((
                    chunk_index,
                    chunk_layer,
                    parent_id,
                    page_start,
                    page_end,
                    chunk_text,
                    retrieval_text,
                    vector,
                    needs_embedding,
                    sync_to_vec_index,
                    timestamp_start,
                )) = iter.next()
                else {
                    break;
                };
                let effective_parent_id = if chunk_layer == Some("child") && parent_id.is_none() {
                    last_parent_rowid
                } else {
                    parent_id
                };
                let _ = Self::insert_chunk_embedding_row_tx(
                    &tx,
                    file_path,
                    chunk_index,
                    chunk_layer,
                    effective_parent_id,
                    page_start,
                    page_end,
                    &chunk_text,
                    &retrieval_text,
                    &vector,
                    needs_embedding,
                    sync_to_vec_index,
                    timestamp_start,
                )?;
                if chunk_layer == Some("parent") {
                    last_parent_rowid = Some(tx.last_insert_rowid());
                }
                n_in_tx += 1;
            }
            tx.commit().map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn upsert_chunk_embedding(
        &self,
        file_path: &str,
        chunk_index: i64,
        chunk_text: &str,
        vector: &[f32],
    ) -> Result<(), String> {
        let _ = self.insert_chunk_embedding_row(
            file_path,
            chunk_index,
            None,
            None,
            None,
            None,
            chunk_text,
            chunk_text,
            vector,
            false,
            true,
            None,
        )?;
        Ok(())
    }

    pub fn delete_embeddings_for_file(&self, file_path: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let _ = conn.execute(
            "DELETE FROM chunk_embeddings_vec WHERE rowid IN (SELECT id FROM chunk_embeddings WHERE file_path = ?1)",
            params![file_path],
        );
        conn.execute(
            "DELETE FROM chunk_embeddings WHERE file_path = ?1",
            params![file_path],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Remove all chunk embeddings whose file_path equals prefix or starts with prefix/ (e.g. when a directory is removed).
    pub fn delete_embeddings_by_path_prefix(&self, prefix: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let like = format!("{}%", prefix);
        let _ = conn.execute(
            "DELETE FROM chunk_embeddings_vec WHERE rowid IN (SELECT id FROM chunk_embeddings WHERE file_path = ?1 OR file_path LIKE ?2)",
            params![prefix, like],
        );
        conn.execute(
            "DELETE FROM chunk_embeddings WHERE file_path = ?1 OR file_path LIKE ?2",
            params![prefix, like],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn search_similar_chunks(
        &self,
        query: &str,
        top_k: usize,
    ) -> Result<Vec<ChunkEmbedding>, String> {
        let query_vec = crate::neural_embed::embed_query(query)?;
        self.search_similar_chunks_with_vec(&query_vec, top_k)
    }

    pub(crate) fn search_similar_chunks_with_vec(
        &self,
        query_vec: &[f32],
        top_k: usize,
    ) -> Result<Vec<ChunkEmbedding>, String> {
        if query_vec.len() != embeddings::EMBEDDING_DIM {
            return Err(format!(
                "query vector length {} must be {}",
                query_vec.len(),
                embeddings::EMBEDDING_DIM
            ));
        }
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let qblob = bytemuck::cast_slice::<f32, u8>(query_vec).to_vec();
        let k = top_k.clamp(1, 200) as i64;
        if let Ok(mut stmt) = conn.prepare(
            "SELECT rowid, distance FROM chunk_embeddings_vec WHERE embedding MATCH ?1 AND k = ?2",
        ) {
            if let Ok(mut rows) = stmt.query(params![qblob, k]) {
                let mut ids: Vec<i64> = Vec::new();
                while let Ok(Some(row)) = rows.next() {
                    let rid: i64 = row.get(0).unwrap_or(0);
                    if rid > 0 {
                        ids.push(rid);
                    }
                }
                if !ids.is_empty() {
                    let placeholders: String = (1..=ids.len())
                        .map(|i| format!("?{i}"))
                        .collect::<Vec<_>>()
                        .join(",");
                    let sql = format!(
                        "SELECT id, file_path, chunk_index, chunk_text, timestamp_start, vector FROM chunk_embeddings WHERE id IN ({placeholders}) AND IFNULL(chunk_layer, '') != 'parent'"
                    );
                    let mut stmt2 = conn.prepare(&sql).map_err(|e| e.to_string())?;
                    let params: Vec<&dyn rusqlite::ToSql> =
                        ids.iter().map(|id| id as &dyn rusqlite::ToSql).collect();
                    let mut out: Vec<ChunkEmbedding> = Vec::new();
                    let mut q = stmt2
                        .query(rusqlite::params_from_iter(params.iter().copied()))
                        .map_err(|e| e.to_string())?;
                    while let Some(row) = q.next().map_err(|e| e.to_string())? {
                        let id: i64 = row.get(0).map_err(|e| e.to_string())?;
                        let file_path: String = row.get(1).map_err(|e| e.to_string())?;
                        let chunk_index: i64 = row.get(2).map_err(|e| e.to_string())?;
                        let chunk_text: String = row.get(3).map_err(|e| e.to_string())?;
                        let timestamp_start: Option<f64> = row.get(4).map_err(|e| e.to_string())?;
                        let blob: Vec<u8> = row.get(5).map_err(|e| e.to_string())?;
                        let expected_len = embeddings::EMBEDDING_DIM * std::mem::size_of::<f32>();
                        let vector: Vec<f32> = if blob.len() != expected_len {
                            vec![0.0; embeddings::EMBEDDING_DIM]
                        } else {
                            bytemuck::cast_slice::<u8, f32>(&blob).to_vec()
                        };
                        let score = embeddings::cosine_similarity(query_vec, &vector);
                        if score < Self::RAG_MIN_SIMILARITY_SCORE {
                            continue;
                        }
                        out.push(ChunkEmbedding {
                            id,
                            file_path,
                            chunk_index,
                            chunk_text,
                            vector,
                            timestamp_start,
                        });
                    }
                    let order: std::collections::HashMap<i64, usize> =
                        ids.iter().enumerate().map(|(i, id)| (*id, i)).collect();
                    out.sort_by_key(|c| order.get(&c.id).copied().unwrap_or(usize::MAX));
                    if !out.is_empty() {
                        return Ok(out);
                    }
                }
            }
        }

        // Fallback: bounded scan + in-Rust cosine (legacy path).
        let min_candidates: usize = 2_000;
        let per_topk: usize = 500;
        let max_candidates: usize = 25_000;
        let candidate_limit = top_k
            .saturating_mul(per_topk)
            .max(min_candidates)
            .min(max_candidates);

        let mut stmt = conn
            .prepare(
                "SELECT id, file_path, chunk_index, chunk_text, timestamp_start, vector FROM chunk_embeddings WHERE IFNULL(chunk_layer, '') != 'parent' ORDER BY id LIMIT ?1",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![candidate_limit as i64], |row| {
                let id: i64 = row.get(0)?;
                let file_path: String = row.get(1)?;
                let chunk_index: i64 = row.get(2)?;
                let chunk_text: String = row.get(3)?;
                let timestamp_start: Option<f64> = row.get(4)?;
                let blob: Vec<u8> = row.get(5)?;
                let expected_len = embeddings::EMBEDDING_DIM * std::mem::size_of::<f32>();
                let vector: Vec<f32> = if blob.len() != expected_len {
                    vec![0.0; embeddings::EMBEDDING_DIM]
                } else {
                    bytemuck::cast_slice::<u8, f32>(&blob).to_vec()
                };
                Ok(ChunkEmbedding {
                    id,
                    file_path,
                    chunk_index,
                    chunk_text,
                    vector,
                    timestamp_start,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut all: Vec<ChunkEmbedding> = Vec::new();
        for row in rows {
            all.push(row.map_err(|e| e.to_string())?);
        }
        let mut with_score: Vec<(f32, ChunkEmbedding)> = all
            .into_iter()
            .map(|c| (embeddings::cosine_similarity(query_vec, &c.vector), c))
            .filter(|(score, _)| *score >= Self::RAG_MIN_SIMILARITY_SCORE)
            .collect();
        with_score.sort_by(|a, b| {
            b.0.partial_cmp(&a.0)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let cap = top_k.clamp(1, 50);
        Ok(with_score
            .into_iter()
            .take(cap)
            .map(|(_, c)| c)
            .collect())
    }

    /// FTS5 child chunk ids for a caller-built `MATCH` string (e.g. token1 OR token2 OR …).
    fn fts_top_child_chunk_ids_for_match_string(
        &self,
        fts_match: &str,
        limit: i64,
    ) -> Result<Vec<i64>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let fts_ready = Self::table_exists(&conn, "library_chunks_fts").unwrap_or(false);
        if !fts_ready || fts_match.trim().is_empty() {
            return Ok(Vec::new());
        }
        let cap = limit.clamp(1, 200);
        let sql = r#"
SELECT c.id
FROM library_chunks_fts
INNER JOIN chunk_embeddings c ON c.id = library_chunks_fts.rowid
LEFT JOIN indexed_files f ON f.path = c.file_path
WHERE library_chunks_fts MATCH ?1
  AND IFNULL(c.chunk_layer, '') != 'parent'
  AND (f.path IS NULL OR f.file_type = 'text' OR f.file_type = 'zim' OR f.file_type = 'learned' OR f.file_type = 'image')
ORDER BY library_chunks_fts.rank ASC
LIMIT ?2
"#;
        let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![fts_match, cap], |row| row.get::<_, i64>(0))
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    fn fts_top_child_chunk_ids(&self, query: &str, limit: i64) -> Result<Vec<i64>, String> {
        let keywords = extract_keywords(query, 20);
        if keywords.is_empty() {
            return Ok(Vec::new());
        }
        let Some(mq) = fts_keywords_to_match_query(&keywords) else {
            return Ok(Vec::new());
        };
        self.fts_top_child_chunk_ids_for_match_string(&mq, limit)
    }

    fn load_hybrid_chunk_rows(
        conn: &Connection,
        ids: &[i64],
    ) -> Result<std::collections::HashMap<i64, HybridChunkRow>, String> {
        use std::collections::HashMap;
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let placeholders = (1..=ids.len())
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT c.id, TRIM(COALESCE(c.retrieval_text, c.chunk_text, '')), c.chunk_text, c.parent_id, c.file_path, c.page_start, c.page_end, f.display_title, c.timestamp_start \
             FROM chunk_embeddings c \
             LEFT JOIN indexed_files f ON f.path = c.file_path \
             WHERE c.id IN ({placeholders})"
        );
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let params: Vec<&dyn rusqlite::ToSql> =
            ids.iter().map(|id| id as &dyn rusqlite::ToSql).collect();
        let mut q = stmt
            .query(rusqlite::params_from_iter(params.iter().copied()))
            .map_err(|e| e.to_string())?;
        let mut map = HashMap::new();
        while let Some(row) = q.next().map_err(|e| e.to_string())? {
            let id: i64 = row.get(0).map_err(|e| e.to_string())?;
            let retrieval_text: String = row.get(1).map_err(|e| e.to_string())?;
            let chunk_text: String = row.get(2).map_err(|e| e.to_string())?;
            let parent_id: Option<i64> = row.get(3).map_err(|e| e.to_string())?;
            let file_path: String = row.get(4).map_err(|e| e.to_string())?;
            let page_start: Option<i64> = row.get(5).map_err(|e| e.to_string())?;
            let _page_end: Option<i64> = row.get(6).map_err(|e| e.to_string())?;
            let display_title: Option<String> = row.get(7).map_err(|e| e.to_string())?;
            let timestamp_start: Option<f64> = row.get(8).map_err(|e| e.to_string())?;
            map.insert(
                id,
                HybridChunkRow {
                    retrieval_text,
                    chunk_text,
                    parent_id,
                    file_path,
                    page_start,
                    display_title,
                    timestamp_start,
                },
            );
        }
        Ok(map)
    }

    /// Parent section text keyed by parent chunk id, for context expansion in hybrid RAG.
    fn load_parent_chunk_texts(
        conn: &Connection,
        parent_ids: &[i64],
    ) -> Result<std::collections::HashMap<i64, String>, String> {
        use std::collections::HashMap;
        if parent_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let placeholders = (1..=parent_ids.len())
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!("SELECT id, chunk_text FROM chunk_embeddings WHERE id IN ({placeholders})");
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let params: Vec<&dyn rusqlite::ToSql> =
            parent_ids.iter().map(|id| id as &dyn rusqlite::ToSql).collect();
        let mut q = stmt
            .query(rusqlite::params_from_iter(params.iter().copied()))
            .map_err(|e| e.to_string())?;
        let mut map = HashMap::new();
        while let Some(row) = q.next().map_err(|e| e.to_string())? {
            let id: i64 = row.get(0).map_err(|e| e.to_string())?;
            let chunk_text: String = row.get(1).map_err(|e| e.to_string())?;
            map.insert(id, chunk_text);
        }
        Ok(map)
    }

    /// Cap how many chunk ids we keep per `file_path` while preserving BM25 / similarity order (breadth).
    fn diversify_child_ids_by_file_path(
        conn: &Connection,
        ordered_ids: &[i64],
        target_len: usize,
        max_per_file: usize,
    ) -> Result<Vec<i64>, String> {
        use std::collections::HashMap;
        if ordered_ids.is_empty() || target_len == 0 {
            return Ok(Vec::new());
        }
        let row_map = Self::load_hybrid_chunk_rows(conn, ordered_ids)?;
        let mut per_file: HashMap<String, usize> = HashMap::new();
        let mut out = Vec::new();
        for id in ordered_ids {
            let Some(row) = row_map.get(id) else {
                continue;
            };
            let c = per_file.entry(row.file_path.clone()).or_insert(0);
            if *c >= max_per_file {
                continue;
            }
            *c += 1;
            out.push(*id);
            if out.len() >= target_len {
                break;
            }
        }
        Ok(out)
    }

    fn diversify_semantic_chunk_ids(
        chunks: &[ChunkEmbedding],
        target_len: usize,
        max_per_file: usize,
    ) -> Vec<i64> {
        use std::collections::HashMap;
        let mut per_file: HashMap<String, usize> = HashMap::new();
        let mut out = Vec::new();
        for c in chunks {
            let e = per_file.entry(c.file_path.clone()).or_insert(0);
            if *e >= max_per_file {
                continue;
            }
            *e += 1;
            out.push(c.id);
            if out.len() >= target_len {
                break;
            }
        }
        out
    }

    /// Hybrid library retrieval: FTS5 (BM25) and vector candidates, fused, then scored by the
    /// cross-encoder reranker and filtered by a relevance cutoff.
    ///
    /// Hits come back in descending relevance. Only chunks that clear the cutoff are returned, so an
    /// empty result means "nothing in the library answers this", not "search failed". Callers decide
    /// how many hits fit the model's context window (see `assemble_library_context` in `lib.rs`).
    pub fn hybrid_search(&self, query: &str) -> Result<Vec<HybridSearchHit>, String> {
        Ok(self
            .hybrid_search_scored(query)?
            .into_iter()
            .filter(|h| h.relevant)
            .map(|h| h.hit)
            .collect())
    }

    /// Same pipeline as [`Self::hybrid_search`] but returns every reranked candidate with its
    /// `relevant` verdict. Used by the library search test box and retrieval diagnostics.
    pub fn hybrid_search_scored(&self, query: &str) -> Result<Vec<ScoredHybridHit>, String> {
        /// Candidates pulled from each arm before fusion.
        const ARM_POOL: i64 = 60;
        /// Candidates sent to the cross-encoder. Each pair costs one CPU inference pass, so this
        /// bounds retrieval latency.
        const RERANK_POOL: usize = 28;
        /// Cap per file inside the rerank pool so one long document cannot crowd out the rest
        /// before the cross-encoder has seen them.
        const MAX_PER_FILE_POOL: usize = 10;
        const RRF_K: f32 = 60.0;

        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }

        // Keyword arm: stopword-filtered terms first, raw tokens as a fallback for short queries.
        let mut fts_ids = self.fts_top_child_chunk_ids(query, ARM_POOL)?;
        if fts_ids.is_empty() {
            let raw = query
                .split(|c: char| !c.is_alphanumeric())
                .filter(|w| w.chars().count() >= 2)
                .map(|w| format!("\"{}\"", w.to_lowercase()))
                .collect::<Vec<_>>()
                .join(" OR ");
            fts_ids = self.fts_top_child_chunk_ids_for_match_string(&raw, ARM_POOL)?;
        }

        // Semantic arm.
        let sem_ids: Vec<i64> = self
            .search_similar_chunks(query, ARM_POOL as usize)
            .unwrap_or_default()
            .into_iter()
            .map(|c| c.id)
            .collect();

        // Reciprocal rank fusion decides which candidates are worth a cross-encoder pass.
        let mut fused: std::collections::HashMap<i64, f32> = std::collections::HashMap::new();
        for (rank, id) in fts_ids.iter().enumerate() {
            *fused.entry(*id).or_insert(0.0) += 1.0 / (RRF_K + rank as f32 + 1.0);
        }
        for (rank, id) in sem_ids.iter().enumerate() {
            *fused.entry(*id).or_insert(0.0) += 1.0 / (RRF_K + rank as f32 + 1.0);
        }
        if fused.is_empty() {
            return Ok(Vec::new());
        }
        let mut fused_order: Vec<(i64, f32)> = fused.into_iter().collect();
        fused_order.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.0.cmp(&b.0))
        });
        let fused_ids: Vec<i64> = fused_order.iter().map(|(id, _)| *id).collect();
        let fused_score: std::collections::HashMap<i64, f32> = fused_order.into_iter().collect();

        let (pool_ids, row_map, parent_texts) = {
            let conn = self.conn.lock().map_err(|e| e.to_string())?;
            let pool_ids = Self::diversify_child_ids_by_file_path(
                &conn,
                &fused_ids,
                RERANK_POOL,
                MAX_PER_FILE_POOL,
            )?;
            let row_map = Self::load_hybrid_chunk_rows(&conn, &pool_ids)?;
            let mut parent_ids: Vec<i64> = row_map.values().filter_map(|r| r.parent_id).collect();
            parent_ids.sort_unstable();
            parent_ids.dedup();
            let parent_texts = Self::load_parent_chunk_texts(&conn, &parent_ids)?;
            (pool_ids, row_map, parent_texts)
        };
        if pool_ids.is_empty() {
            return Ok(Vec::new());
        }

        // Score. With the real cross-encoder the score is a relevance logit; without it we keep the
        // fused rank so results are still ordered sensibly instead of by a hash.
        let reranked = crate::rerank::global_is_onnx_loaded();
        let scores: Vec<f32> = if reranked {
            let passages: Vec<String> = pool_ids
                .iter()
                .map(|id| {
                    row_map
                        .get(id)
                        .map(|r| r.retrieval_text.clone())
                        .unwrap_or_default()
                })
                .collect();
            let s = crate::rerank::score_pairs_global(query, &passages)?;
            if s.len() != pool_ids.len() {
                return Err("reranker score count mismatch".to_string());
            }
            s
        } else {
            pool_ids
                .iter()
                .map(|id| fused_score.get(id).copied().unwrap_or(0.0))
                .collect()
        };

        let mut ranked: Vec<(f32, i64)> = scores.into_iter().zip(pool_ids.into_iter()).collect();
        ranked.sort_by(|a, b| {
            b.0.partial_cmp(&a.0)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let top_score = ranked.first().map(|r| r.0).unwrap_or(f32::MIN);

        let mut out: Vec<ScoredHybridHit> = Vec::new();
        let mut seen_text: std::collections::HashSet<String> = std::collections::HashSet::new();
        for (position, (score, id)) in ranked.into_iter().enumerate() {
            let Some(row) = row_map.get(&id) else {
                continue;
            };
            // Near-duplicate chunks (same opening text) add nothing to the prompt.
            let key: String = row
                .chunk_text
                .chars()
                .filter(|c| !c.is_whitespace())
                .take(120)
                .flat_map(|c| c.to_lowercase())
                .collect();
            if !seen_text.insert(key) {
                continue;
            }
            let relevant = out.iter().filter(|h| h.relevant).count() < HYBRID_SEARCH_TOP_CHUNKS
                && if reranked {
                    hybrid_rerank_score_is_relevant(score, top_score)
                } else {
                    position < HYBRID_SEARCH_TOP_CHUNKS_NO_RERANKER
                };
            out.push(ScoredHybridHit {
                hit: HybridSearchHit {
                    expanded_text: row.chunk_text.clone(),
                    display_title: row.display_title.clone(),
                    page_number: row.page_start,
                    file_path: row.file_path.clone(),
                    timestamp_start: row.timestamp_start,
                    score,
                    parent_id: row.parent_id,
                    parent_text: row
                        .parent_id
                        .and_then(|pid| parent_texts.get(&pid).cloned()),
                },
                relevant,
                reranked,
            });
        }
        Ok(out)
    }

    /// Distinct library file paths surfaced by BM25 (in order) then vector search, for source pills / multi-doc awareness.
    pub fn distinct_library_source_paths(
        &self,
        query: &str,
        max_paths: usize,
    ) -> Result<Vec<(String, Option<String>)>, String> {
        use std::collections::{HashMap, HashSet};
        let mut seen: HashSet<String> = HashSet::new();
        let mut out: Vec<(String, Option<String>)> = Vec::new();
        if max_paths == 0 {
            return Ok(out);
        }
        let fts_raw = self.fts_top_child_chunk_ids(query, 120)?;
        let fts_pick = if fts_raw.is_empty() {
            Vec::new()
        } else {
            let conn = self.conn.lock().map_err(|e| e.to_string())?;
            Self::diversify_child_ids_by_file_path(
                &conn,
                &fts_raw,
                max_paths.saturating_mul(4).max(16),
                1,
            )?
        };
        if !fts_pick.is_empty() {
            let conn = self.conn.lock().map_err(|e| e.to_string())?;
            let row_map = Self::load_hybrid_chunk_rows(&conn, &fts_pick)?;
            drop(conn);
            for id in fts_pick {
                if out.len() >= max_paths {
                    break;
                }
                if let Some(r) = row_map.get(&id) {
                    if seen.insert(r.file_path.clone()) {
                        out.push((r.file_path.clone(), r.display_title.clone()));
                    }
                }
            }
        }
        if out.len() >= max_paths {
            return Ok(out);
        }
        let sem = self.search_similar_chunks(query, 120).unwrap_or_default();
        let sem_ids = Self::diversify_semantic_chunk_ids(&sem, 48, 1);
        let by_id: HashMap<i64, &ChunkEmbedding> = sem.iter().map(|c| (c.id, c)).collect();
        for id in sem_ids {
            if out.len() >= max_paths {
                break;
            }
            let Some(c) = by_id.get(&id) else {
                continue;
            };
            if seen.insert(c.file_path.clone()) {
                let title = self
                    .get_by_path(&c.file_path)
                    .ok()
                    .flatten()
                    .and_then(|f| f.display_title);
                out.push((c.file_path.clone(), title));
            }
        }
        Ok(out)
    }

    /// Recent indexed files that have child chunks, excluding paths already shown—widens "Sources" when one doc dominates the query.
    pub fn distinct_child_chunk_file_paths_breadth(
        &self,
        exclude: &std::collections::HashSet<String>,
        max_add: usize,
    ) -> Result<Vec<(String, Option<String>)>, String> {
        use std::collections::HashSet;
        if max_add == 0 {
            return Ok(Vec::new());
        }
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT c.file_path, f.display_title FROM chunk_embeddings c \
                 LEFT JOIN indexed_files f ON f.path = c.file_path \
                 WHERE IFNULL(c.chunk_layer, '') != 'parent' \
                 ORDER BY c.id DESC",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
        let mut seen_path: HashSet<String> = HashSet::new();
        let mut out = Vec::new();
        while let Some(row) = rows.next().map_err(|e| e.to_string())? {
            let path: String = row.get(0).map_err(|e| e.to_string())?;
            if exclude.contains(&path) || !seen_path.insert(path.clone()) {
                continue;
            }
            let title: Option<String> = row.get(1).map_err(|e| e.to_string())?;
            out.push((path, title));
            if out.len() >= max_add {
                break;
            }
        }
        Ok(out)
    }

    /// First child chunk text for a file (for breadth snippets when retrieval stays on one document).
    pub fn first_child_chunk_text_preview(
        &self,
        file_path: &str,
        max_chars: usize,
    ) -> Result<String, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let text: String = conn
            .query_row(
                "SELECT TRIM(COALESCE(retrieval_text, chunk_text, '')) FROM chunk_embeddings \
                 WHERE file_path = ?1 AND IFNULL(chunk_layer, '') != 'parent' \
                 ORDER BY chunk_index ASC LIMIT 1",
                params![file_path],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        let cap = max_chars.clamp(80, 8000);
        let n = text.chars().count();
        if n > cap {
            Ok(text.chars().take(cap).collect::<String>() + "…")
        } else {
            Ok(text)
        }
    }

    pub fn list_logbook_entries(&self, include_tactical: bool) -> Result<Vec<LogbookEntry>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let sql = if include_tactical {
            "SELECT id, body, created_at, from_node_id, author_name, author_occupation, user_id, author_rank_level, author_division, parent_entry_id, notebook FROM logbook_entries ORDER BY created_at DESC"
        } else {
            "SELECT id, body, created_at, from_node_id, author_name, author_occupation, user_id, author_rank_level, author_division, parent_entry_id, notebook FROM logbook_entries WHERE lower(COALESCE(notebook, 'general')) != 'tactical' ORDER BY created_at DESC"
        };
        let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                let from_raw: Option<i64> = row.get(3)?;
                Ok(LogbookEntry {
                    id: row.get(0)?,
                    body: row.get(1)?,
                    created_at: row.get(2)?,
                    from_node_id: from_raw.and_then(|n| u32::try_from(n).ok()),
                    author_name: row.get(4).unwrap_or_default(),
                    author_occupation: row.get(5).unwrap_or_default(),
                    user_id: row.get(6)?,
                    author_rank_level: row.get(7)?,
                    author_division: row.get(8)?,
                    author_directorate: None,
                    author_tier: None,
                    author_job_title: None,
                    map_lat: None,
                    map_lon: None,
                    saved_place_id: None,
                    location_text: None,
                    parent_entry_id: row.get(9)?,
                    notebook: Self::logbook_notebook_cell(row, 10)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    fn logbook_notebook_cell(row: &rusqlite::Row, idx: usize) -> Result<String, rusqlite::Error> {
        match row.get::<_, Option<String>>(idx)? {
            Some(s) if !s.is_empty() => Ok(s),
            _ => Ok("general".to_string()),
        }
    }

    pub fn list_logbook_entries_since(
        &self,
        since_created_at: i64,
        limit: usize,
    ) -> Result<Vec<LogbookEntry>, String> {
        let cap = limit.clamp(1, 128);
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, body, created_at, from_node_id, author_name, author_occupation, user_id, author_rank_level, author_division, parent_entry_id, notebook
                 FROM logbook_entries
                 WHERE created_at > ?1
                   AND lower(COALESCE(notebook, 'general')) != 'tactical'
                 ORDER BY created_at DESC
                 LIMIT ?2",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![since_created_at, cap as i64], |row| {
                let from_raw: Option<i64> = row.get(3)?;
                Ok(LogbookEntry {
                    id: row.get(0)?,
                    body: row.get(1)?,
                    created_at: row.get(2)?,
                    from_node_id: from_raw.and_then(|n| u32::try_from(n).ok()),
                    author_name: row.get(4).unwrap_or_default(),
                    author_occupation: row.get(5).unwrap_or_default(),
                    user_id: row.get(6)?,
                    author_rank_level: row.get(7)?,
                    author_division: row.get(8)?,
                    author_directorate: None,
                    author_tier: None,
                    author_job_title: None,
                    map_lat: None,
                    map_lon: None,
                    saved_place_id: None,
                    location_text: None,
                    parent_entry_id: row.get(9)?,
                    notebook: Self::logbook_notebook_cell(row, 10)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn get_max_logbook_created_at(&self) -> Result<Option<i64>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.query_row("SELECT MAX(created_at) FROM logbook_entries", [], |row| {
            row.get::<_, Option<i64>>(0)
        })
        .map_err(|e| e.to_string())
    }

    /// Recent entries for answering a mesh sync request (newest first).
    pub fn list_logbook_recent_for_mesh_share(
        &self,
        limit: usize,
    ) -> Result<Vec<LogbookEntry>, String> {
        let cap = limit.clamp(1, 32);
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, body, created_at, from_node_id, author_name, author_occupation, user_id, author_rank_level, author_division, parent_entry_id, notebook FROM logbook_entries WHERE lower(COALESCE(notebook, 'general')) != 'tactical' ORDER BY created_at DESC LIMIT ?1",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![cap as i64], |row| {
                let from_raw: Option<i64> = row.get(3)?;
                Ok(LogbookEntry {
                    id: row.get(0)?,
                    body: row.get(1)?,
                    created_at: row.get(2)?,
                    from_node_id: from_raw.and_then(|n| u32::try_from(n).ok()),
                    author_name: row.get(4).unwrap_or_default(),
                    author_occupation: row.get(5).unwrap_or_default(),
                    user_id: row.get(6)?,
                    author_rank_level: row.get(7)?,
                    author_division: row.get(8)?,
                    author_directorate: None,
                    author_tier: None,
                    author_job_title: None,
                    map_lat: None,
                    map_lon: None,
                    saved_place_id: None,
                    location_text: None,
                    parent_entry_id: row.get(9)?,
                    notebook: Self::logbook_notebook_cell(row, 10)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn get_logbook_entry(&self, id: &str) -> Result<Option<LogbookEntry>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, body, created_at, from_node_id, author_name, author_occupation, user_id, author_rank_level, author_division, parent_entry_id, notebook FROM logbook_entries WHERE id = ?1 LIMIT 1",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query(params![id]).map_err(|e| e.to_string())?;
        if let Some(row) = rows.next().map_err(|e| e.to_string())? {
            let from_raw: Option<i64> = row.get(3).map_err(|e| e.to_string())?;
            Ok(Some(LogbookEntry {
                id: row.get(0).map_err(|e| e.to_string())?,
                body: row.get(1).map_err(|e| e.to_string())?,
                created_at: row.get(2).map_err(|e| e.to_string())?,
                from_node_id: from_raw.and_then(|n| u32::try_from(n).ok()),
                author_name: row.get(4).unwrap_or_default(),
                author_occupation: row.get(5).unwrap_or_default(),
                user_id: row.get(6).map_err(|e| e.to_string())?,
                author_rank_level: row.get(7).map_err(|e| e.to_string())?,
                author_division: row.get(8).map_err(|e| e.to_string())?,
                author_directorate: None,
                author_tier: None,
                author_job_title: None,
                map_lat: None,
                map_lon: None,
                saved_place_id: None,
                location_text: None,
                parent_entry_id: row.get(9).map_err(|e| e.to_string())?,
                notebook: Self::logbook_notebook_cell(&row, 10).map_err(|e| e.to_string())?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn insert_logbook_local(
        &self,
        id: &str,
        body: &str,
        created_at: i64,
        author_name: &str,
        author_occupation: &str,
        parent_entry_id: Option<&str>,
        notebook: &str,
    ) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO logbook_entries (id, body, created_at, from_node_id, author_name, author_occupation, parent_entry_id, notebook) VALUES (?1, ?2, ?3, NULL, ?4, ?5, ?6, ?7)",
            params![id, body, created_at, author_name, author_occupation, parent_entry_id, notebook],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn insert_logistics_library_entry(
        &self,
        id: &str,
        body: &str,
        created_at: i64,
        author_name: &str,
        author_occupation: &str,
        user_id: Option<&str>,
        author_rank_level: Option<i32>,
        author_division: Option<&str>,
        author_directorate: Option<&str>,
        author_tier: Option<&str>,
        author_job_title: Option<&str>,
        map_lat: Option<f64>,
        map_lon: Option<f64>,
        saved_place_id: Option<&str>,
        location_text: Option<&str>,
        notebook: &str,
    ) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO logbook_entries (id, body, created_at, from_node_id, author_name, author_occupation, user_id, author_rank_level, author_division, author_directorate, author_tier, author_job_title, map_lat, map_lon, saved_place_id, location_text, parent_entry_id, notebook) VALUES (?1, ?2, ?3, NULL, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, NULL, ?16)",
            params![
                id,
                body,
                created_at,
                author_name,
                author_occupation,
                user_id,
                author_rank_level,
                author_division,
                author_directorate,
                author_tier,
                author_job_title,
                map_lat,
                map_lon,
                saved_place_id,
                location_text,
                notebook
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Returns `true` if a new row was inserted.
    pub fn try_insert_logbook_from_mesh(
        &self,
        id: &str,
        body: &str,
        created_at: i64,
        from_node_id: u32,
    ) -> Result<bool, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let n = conn
            .execute(
                "INSERT OR IGNORE INTO logbook_entries (id, body, created_at, from_node_id) VALUES (?1, ?2, ?3, ?4)",
                params![id, body, created_at, from_node_id as i64],
            )
            .map_err(|e| e.to_string())?;
        Ok(n > 0)
    }

    /// LAN merge for logbook entries. Preserves `NULL` / missing authorship if `from_node_id` is `None`.
    /// Returns `true` if a new row was inserted.
    pub fn try_insert_logbook_from_lan(&self, entry: &LogbookEntry) -> Result<bool, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let from_raw: Option<i64> = entry.from_node_id.map(|n| n as i64);
        let nb = entry.notebook.trim();
        let notebook = if nb.is_empty() {
            "general"
        } else {
            nb
        };
        let n = conn
            .execute(
                "INSERT OR IGNORE INTO logbook_entries (id, body, created_at, from_node_id, author_name, author_occupation, user_id, author_rank_level, author_division, author_directorate, author_tier, author_job_title, map_lat, map_lon, saved_place_id, location_text, parent_entry_id, notebook) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
                params![
                    entry.id.trim(),
                    entry.body.as_str(),
                    entry.created_at,
                    from_raw,
                    entry.author_name.as_str(),
                    entry.author_occupation.as_str(),
                    entry.user_id.as_deref(),
                    entry.author_rank_level,
                    entry.author_division.as_deref(),
                    entry.author_directorate.as_deref(),
                    entry.author_tier.as_deref(),
                    entry.author_job_title.as_deref(),
                    entry.map_lat,
                    entry.map_lon,
                    entry.saved_place_id.as_deref(),
                    entry.location_text.as_deref(),
                    entry.parent_entry_id.as_deref(),
                    notebook,
                ],
            )
            .map_err(|e| e.to_string())?;
        Ok(n > 0)
    }

    pub fn delete_logbook_entry(&self, id: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM logbook_entries WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn list_calendar_events(
        &self,
        range_start: Option<i64>,
        range_end: Option<i64>,
        include_deleted: bool,
    ) -> Result<Vec<CalendarEvent>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, title, notes, start_ts, end_ts, all_day, location_label, linked_marker_id,
                        participant_names_json, reminder_minutes_before, recurrence_rule, category, source, updated_at,
                        origin_node_id, deleted
                 FROM calendar_events
                 WHERE (?1 IS NULL OR end_ts >= ?1)
                   AND (?2 IS NULL OR start_ts <= ?2)
                   AND (?3 = 1 OR deleted = 0)
                 ORDER BY start_ts ASC, updated_at DESC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(
                params![
                    range_start,
                    range_end,
                    if include_deleted { 1_i64 } else { 0_i64 }
                ],
                |row| {
                    let participants_json: String = row.get(8)?;
                    let participants =
                        serde_json::from_str::<Vec<String>>(&participants_json).unwrap_or_default();
                    let origin_raw: Option<i64> = row.get(14)?;
                    let deleted_raw: i64 = row.get(15)?;
                    Ok(CalendarEvent {
                        id: row.get(0)?,
                        title: row.get(1)?,
                        notes: row.get(2)?,
                        start_ts: row.get(3)?,
                        end_ts: row.get(4)?,
                        all_day: row.get::<_, i64>(5)? != 0,
                        location_label: row.get(6)?,
                        linked_marker_id: row.get(7)?,
                        participant_names: participants,
                        reminder_minutes_before: row.get(9)?,
                        recurrence_rule: row.get(10)?,
                        category: row.get(11)?,
                        source: row.get(12)?,
                        updated_at: row.get(13)?,
                        origin_node_id: origin_raw.and_then(|n| u32::try_from(n).ok()),
                        deleted: deleted_raw != 0,
                    })
                },
            )
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn list_calendar_recent_for_mesh_share(
        &self,
        limit: usize,
    ) -> Result<Vec<CalendarEvent>, String> {
        let cap = limit.clamp(1, 48);
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, title, notes, start_ts, end_ts, all_day, location_label, linked_marker_id,
                        participant_names_json, reminder_minutes_before, recurrence_rule, category, source, updated_at,
                        origin_node_id, deleted
                 FROM calendar_events
                 ORDER BY updated_at DESC
                 LIMIT ?1",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![cap as i64], |row| {
                let participants_json: String = row.get(8)?;
                let participants =
                    serde_json::from_str::<Vec<String>>(&participants_json).unwrap_or_default();
                let origin_raw: Option<i64> = row.get(14)?;
                let deleted_raw: i64 = row.get(15)?;
                Ok(CalendarEvent {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    notes: row.get(2)?,
                    start_ts: row.get(3)?,
                    end_ts: row.get(4)?,
                    all_day: row.get::<_, i64>(5)? != 0,
                    location_label: row.get(6)?,
                    linked_marker_id: row.get(7)?,
                    participant_names: participants,
                    reminder_minutes_before: row.get(9)?,
                    recurrence_rule: row.get(10)?,
                    category: row.get(11)?,
                    source: row.get(12)?,
                    updated_at: row.get(13)?,
                    origin_node_id: origin_raw.and_then(|n| u32::try_from(n).ok()),
                    deleted: deleted_raw != 0,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn upsert_calendar_event(&self, event: &CalendarEvent) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let participants_json =
            serde_json::to_string(&event.participant_names).map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO calendar_events (
                id, title, notes, start_ts, end_ts, all_day, location_label, linked_marker_id,
                participant_names_json, reminder_minutes_before, recurrence_rule, category, source, updated_at,
                origin_node_id, deleted
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8,
                ?9, ?10, ?11, ?12, ?13, ?14,
                ?15, ?16
             )
             ON CONFLICT(id) DO UPDATE SET
                title = excluded.title,
                notes = excluded.notes,
                start_ts = excluded.start_ts,
                end_ts = excluded.end_ts,
                all_day = excluded.all_day,
                location_label = excluded.location_label,
                linked_marker_id = excluded.linked_marker_id,
                participant_names_json = excluded.participant_names_json,
                reminder_minutes_before = excluded.reminder_minutes_before,
                recurrence_rule = excluded.recurrence_rule,
                category = excluded.category,
                source = excluded.source,
                updated_at = excluded.updated_at,
                origin_node_id = excluded.origin_node_id,
                deleted = excluded.deleted",
            params![
                event.id.trim(),
                event.title.trim(),
                event.notes.as_deref(),
                event.start_ts,
                event.end_ts,
                if event.all_day { 1_i64 } else { 0_i64 },
                event.location_label.as_deref(),
                event.linked_marker_id.as_deref(),
                participants_json,
                event.reminder_minutes_before,
                event.recurrence_rule.as_deref(),
                event.category.as_deref(),
                event.source.trim(),
                event.updated_at,
                event.origin_node_id.map(|n| n as i64),
                if event.deleted { 1_i64 } else { 0_i64 },
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Mesh merge (LWW): accepts inbound row only if it's newer than local row with same id.
    /// Returns true when DB changed.
    pub fn upsert_calendar_event_from_mesh(&self, event: &CalendarEvent) -> Result<bool, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let existing_updated_at: Option<i64> = conn
            .query_row(
                "SELECT updated_at FROM calendar_events WHERE id = ?1 LIMIT 1",
                params![event.id.trim()],
                |row| row.get(0),
            )
            .ok();
        if let Some(local_updated_at) = existing_updated_at {
            if local_updated_at > event.updated_at {
                return Ok(false);
            }
        }
        drop(conn);
        self.upsert_calendar_event(event)?;
        Ok(true)
    }

    /// Soft-delete to preserve mesh tombstones.
    pub fn tombstone_calendar_event(&self, id: &str, updated_at: i64) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let n = conn
            .execute(
                "UPDATE calendar_events
                 SET deleted = 1, updated_at = ?2
                 WHERE id = ?1",
                params![id.trim(), updated_at],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            conn.execute(
                "INSERT INTO calendar_events (
                    id, title, notes, start_ts, end_ts, all_day, location_label, linked_marker_id,
                    participant_names_json, reminder_minutes_before, recurrence_rule, category, source, updated_at,
                    origin_node_id, deleted
                 ) VALUES (
                    ?1, '(deleted)', NULL, ?2, ?2, 1, NULL, NULL,
                    '[]', NULL, NULL, NULL, 'user', ?2, NULL, 1
                 )",
                params![id.trim(), updated_at],
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn list_academy_progress(&self) -> Result<Vec<AcademyProgressRow>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT lesson_id, completed_at FROM academy_progress ORDER BY completed_at DESC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok(AcademyProgressRow {
                    lesson_id: row.get(0)?,
                    completed_at: row.get(1)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let out: Result<Vec<_>, rusqlite::Error> = rows.collect();
        drop(stmt);
        drop(conn);
        out.map_err(|e| e.to_string())
    }

    pub fn mesh_profile_upsert(
        &self,
        node_id: i64,
        username: &str,
        counter_sign: &str,
        rank_level: i32,
    ) -> Result<MeshProfile, String> {
        let username = username.trim();
        let counter_sign = counter_sign.trim();
        if username.is_empty() || counter_sign.is_empty() {
            return Err("Username and counter-sign are required.".to_string());
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let existing: Option<String> = conn
            .query_row(
                "SELECT id FROM mesh_profiles WHERE node_id = ?1 LIMIT 1",
                params![node_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let id = existing.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        conn.execute(
            "INSERT INTO mesh_profiles (id, node_id, username, counter_sign, created_at, sos_state, is_registered, rank_level)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, 1, ?6)
             ON CONFLICT(node_id) DO UPDATE SET
               username = excluded.username,
               counter_sign = excluded.counter_sign,
               is_registered = 1,
               rank_level = excluded.rank_level",
            params![id.clone(), node_id, username, counter_sign, now, rank_level as i64],
        )
        .map_err(|e| e.to_string())?;
        drop(conn);
        self.mesh_profile_get_by_node(node_id)?.ok_or_else(|| "Profile upsert failed.".to_string())
    }

    pub fn mesh_profile_insert_sos_unknown_node(&self, node_id: i64) -> Result<MeshProfile, String> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let username = format!("UNKNOWN-{}", node_id);
        let counter_sign: String = uuid::Uuid::new_v4()
            .to_string()
            .chars()
            .filter(|c| *c != '-')
            .take(10)
            .collect();
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let existing: Option<String> = conn
            .query_row(
                "SELECT id FROM mesh_profiles WHERE node_id = ?1 LIMIT 1",
                params![node_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if existing.is_some() {
            return Err("Mesh profile already exists for this node.".to_string());
        }
        let id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO mesh_profiles (id, node_id, username, counter_sign, created_at, sos_state, is_registered, rank_level)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, 1)",
            params![
                id,
                node_id,
                username,
                counter_sign,
                now,
                SOS_STATE_ACTIVE as i64
            ],
        )
        .map_err(|e| e.to_string())?;
        drop(conn);
        self.mesh_profile_get_by_node(node_id)?.ok_or_else(|| "Emergency profile insert failed.".to_string())
    }

    pub fn mesh_profile_set_sos_state(&self, node_id: i64, state: i32) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let n = conn
            .execute(
                "UPDATE mesh_profiles SET sos_state = ?2 WHERE node_id = ?1",
                params![node_id, state],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("No mesh profile for that node.".to_string());
        }
        Ok(())
    }

    pub fn mesh_profile_sos_state(&self, node_id: i64) -> i32 {
        self.mesh_profile_get_by_node(node_id)
            .ok()
            .flatten()
            .map(|p| p.sos_state)
            .unwrap_or(SOS_STATE_NORMAL)
    }

    pub fn mesh_emergency_operator_resolve(
        &self,
        node_id: i64,
        accept_clear: bool,
    ) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        if accept_clear {
            conn.execute(
                "UPDATE mesh_profiles SET sos_state = ?2 WHERE node_id = ?1",
                params![node_id, SOS_STATE_NORMAL],
            )
        } else {
            conn.execute(
                "UPDATE mesh_profiles SET sos_state = ?2 WHERE node_id = ?1 AND sos_state = ?3",
                params![node_id, SOS_STATE_ACTIVE, SOS_STATE_SUSPENDED],
            )
        }
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn list_mesh_profiles_sos_non_normal(&self) -> Result<Vec<MeshProfile>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, node_id, username, counter_sign, created_at, sos_state, is_registered, rank_level
                 FROM mesh_profiles WHERE sos_state != 0 ORDER BY sos_state DESC, username ASC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok(MeshProfile {
                    id: row.get(0)?,
                    node_id: row.get(1)?,
                    username: row.get(2)?,
                    counter_sign: row.get(3)?,
                    created_at: row.get(4)?,
                    sos_state: row.get::<_, i64>(5)? as i32,
                    is_registered: row.get::<_, i64>(6)? != 0,
                    rank_level: row.get::<_, i64>(7)? as i32,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn upsert_mesh_node_position_live(
        &self,
        node_num: u32,
        lat: f64,
        lon: f64,
        now_ms: i64,
        label: &str,
    ) -> Result<(), String> {
        let node_id = format!("{}", node_num);
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO mesh_node_locations (node_id, label, lat, lon, last_seen_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(node_id) DO UPDATE SET
               lat = excluded.lat,
               lon = excluded.lon,
               last_seen_at = excluded.last_seen_at,
               label = CASE
                 WHEN length(trim(excluded.label)) > 0 THEN excluded.label
                 ELSE mesh_node_locations.label
               END",
            params![node_id, label.trim(), lat, lon, now_ms],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn mesh_last_position_for_node(
        &self,
        node_num: u32,
    ) -> Result<Option<(f64, f64, i64)>, String> {
        let node_id = format!("{}", node_num);
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let r: Option<(f64, f64, i64)> = conn
            .query_row(
                "SELECT lat, lon, last_seen_at FROM mesh_node_locations WHERE node_id = ?1 LIMIT 1",
                params![node_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        Ok(r)
    }

    pub fn mesh_profile_delete_by_id(&self, id: &str) -> Result<bool, String> {
        let id = id.trim();
        if id.is_empty() {
            return Err("Profile id is required.".to_string());
        }
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let n = conn
            .execute("DELETE FROM mesh_profiles WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(n > 0)
    }

    pub fn mesh_profile_list(&self) -> Result<Vec<MeshProfile>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, node_id, username, counter_sign, created_at, sos_state, is_registered, rank_level
                 FROM mesh_profiles ORDER BY username ASC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok(MeshProfile {
                    id: row.get(0)?,
                    node_id: row.get(1)?,
                    username: row.get(2)?,
                    counter_sign: row.get(3)?,
                    created_at: row.get(4)?,
                    sos_state: row.get::<_, i64>(5)? as i32,
                    is_registered: row.get::<_, i64>(6)? != 0,
                    rank_level: row.get::<_, i64>(7)? as i32,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn mesh_profile_get_by_node(&self, node_id: i64) -> Result<Option<MeshProfile>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, node_id, username, counter_sign, created_at, sos_state, is_registered, rank_level
                 FROM mesh_profiles WHERE node_id = ?1 LIMIT 1",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query(params![node_id]).map_err(|e| e.to_string())?;
        if let Some(row) = rows.next().map_err(|e| e.to_string())? {
            let row = row;
            Ok(Some(MeshProfile {
                id: row.get(0).map_err(|e| e.to_string())?,
                node_id: row.get(1).map_err(|e| e.to_string())?,
                username: row.get(2).map_err(|e| e.to_string())?,
                counter_sign: row.get(3).map_err(|e| e.to_string())?,
                created_at: row.get(4).map_err(|e| e.to_string())?,
                sos_state: row
                    .get::<_, i64>(5)
                    .map_err(|e| e.to_string())? as i32,
                is_registered: row.get::<_, i64>(6).map_err(|e| e.to_string())? != 0,
                rank_level: row
                    .get::<_, i64>(7)
                    .map_err(|e| e.to_string())? as i32,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn mesh_chat_insert(
        &self,
        id: &str,
        node_id: i64,
        sender_type: &str,
        message_type: &str,
        content: &str,
        created_at: i64,
    ) -> Result<(), String> {
        with_sql_connection_busy_retries(&self.conn, |conn| {
            conn.execute(
                "INSERT INTO mesh_chats (id, node_id, sender_type, message_type, content, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![id, node_id, sender_type, message_type, content, created_at],
            )
        })?;
        Ok(())
    }

    pub fn get_or_create_lan_chat_secret_b64(&self) -> Result<String, String> {
        const KEY: &str = "lan_chat_secret_b64";
        let existing: Option<String> = with_sql_connection_busy_retries(&self.conn, |conn| {
            conn.query_row(
                "SELECT value FROM app_meta WHERE key = ?1 LIMIT 1",
                params![KEY],
                |r| r.get(0),
            )
            .optional()
        })?;
        if let Some(v) = existing {
            if !v.trim().is_empty() {
                return Ok(v);
            }
        }
        let mut bytes = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut bytes);
        let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
        with_sql_connection_busy_retries(&self.conn, |conn| {
            conn.execute(
                "INSERT INTO app_meta (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![KEY, b64],
            )
        })?;
        Ok(b64)
    }

    pub fn insert_lan_chat_message(
        &self,
        row: &crate::lan_chat::LanChatMessageRow,
    ) -> Result<(), String> {
        with_sql_connection_busy_retries(&self.conn, |conn| {
            conn.execute(
                "INSERT INTO lan_chat_messages (id, sender_node_id, sender_label, content, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    row.id,
                    row.sender_node_id as i64,
                    row.sender_label,
                    row.content,
                    row.created_at
                ],
            )
        })?;
        Ok(())
    }

    pub fn list_lan_chat_messages(
        &self,
        limit: i64,
    ) -> Result<Vec<crate::lan_chat::LanChatMessageRow>, String> {
        let cap = limit.clamp(1, 2000);
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, sender_node_id, sender_label, content, created_at
                 FROM lan_chat_messages ORDER BY created_at ASC LIMIT ?1",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![cap], |r| {
                Ok(crate::lan_chat::LanChatMessageRow {
                    id: r.get(0)?,
                    sender_node_id: r.get::<_, i64>(1)? as u32,
                    sender_label: r.get(2)?,
                    content: r.get(3)?,
                    created_at: r.get(4)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    /// Deletes LAN chat rows older than `days` (field laptop DB hygiene).
    pub fn prune_lan_chat_messages_older_than_days(&self, days: i64) -> Result<u64, String> {
        let days = days.clamp(1, 3650);
        let cutoff = chrono::Utc::now().timestamp().saturating_sub(days.saturating_mul(86400));
        let n = with_sql_connection_busy_retries(&self.conn, |conn| {
            conn.execute(
                "DELETE FROM lan_chat_messages WHERE created_at < ?1",
                params![cutoff],
            )
        })?;
        Ok(n as u64)
    }

    pub fn mesh_chats_remote_count_for_node(&self, node_id: i64) -> Result<i64, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let c: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM mesh_chats WHERE node_id = ?1 AND sender_type = 'remote'",
                params![node_id],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        Ok(c)
    }

    pub fn list_mesh_chats_for_node(&self, node_id: i64, limit: usize) -> Result<Vec<MeshChatRow>, String> {
        let cap = limit.clamp(1, 500) as i64;
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, node_id, sender_type, message_type, content, created_at
                 FROM mesh_chats WHERE node_id = ?1 ORDER BY created_at ASC LIMIT ?2",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![node_id, cap], |row| {
                Ok(MeshChatRow {
                    id: row.get(0)?,
                    node_id: row.get(1)?,
                    sender_type: row.get(2)?,
                    message_type: row.get(3)?,
                    content: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn list_mesh_network_nodes(&self) -> Result<Vec<MeshNetworkNodeSummary>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT c.node_id, MAX(c.created_at) AS last_at,
                        (SELECT p.username FROM mesh_profiles p WHERE p.node_id = c.node_id LIMIT 1) AS uname
                 FROM mesh_chats c
                 GROUP BY c.node_id
                 ORDER BY last_at DESC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                let node_id: i64 = row.get(0)?;
                let last_at: i64 = row.get(1)?;
                let uname: Option<String> = row.get(2)?;
                Ok(MeshNetworkNodeSummary {
                    node_id: node_id as u32,
                    username: uname,
                    last_message_at: last_at,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn mark_academy_lesson_complete(&self, lesson_id: &str) -> Result<(), String> {
        let id = lesson_id.trim();
        if id.is_empty() {
            return Err("Lesson id is required.".to_string());
        }
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO academy_progress (lesson_id, completed_at) VALUES (?1, ?2)
             ON CONFLICT(lesson_id) DO UPDATE SET completed_at = excluded.completed_at",
            params![id, now_ms],
        )
        .map_err(|e| e.to_string())?;
        drop(conn);
        Ok(())
    }

    // --- Map overhaul: saved places, blockages, AI staging, routing regions ---

    pub fn insert_saved_place(&self, row: &SavedPlaceRow) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO saved_places (id, name, lat, lng, icon, color, scope, owner_personnel_id, created_at, updated_at, deleted_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                row.id,
                row.name,
                row.lat,
                row.lng,
                row.icon,
                row.color,
                row.scope,
                row.owner_personnel_id,
                row.created_at,
                row.updated_at,
                row.deleted_at,
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn update_saved_place(&self, row: &SavedPlaceRow) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let n = conn
            .execute(
                "UPDATE saved_places SET name=?2, lat=?3, lng=?4, icon=?5, color=?6, scope=?7, updated_at=?8, deleted_at=?9
                 WHERE id=?1",
                params![
                    row.id,
                    row.name,
                    row.lat,
                    row.lng,
                    row.icon,
                    row.color,
                    row.scope,
                    row.updated_at,
                    row.deleted_at,
                ],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("Saved place not found.".to_string());
        }
        Ok(())
    }

    pub fn list_saved_places_for_user(
        &self,
        owner_id: &str,
    ) -> Result<Vec<SavedPlaceRow>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, name, lat, lng, icon, color, scope, owner_personnel_id, created_at, updated_at, deleted_at
                 FROM saved_places
                 WHERE (owner_personnel_id = ?1 OR scope = 'shared') AND deleted_at IS NULL
                 ORDER BY name COLLATE NOCASE",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![owner_id], |row| {
                Ok(SavedPlaceRow {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    lat: row.get(2)?,
                    lng: row.get(3)?,
                    icon: row.get(4)?,
                    color: row.get(5)?,
                    scope: row.get(6)?,
                    owner_personnel_id: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                    deleted_at: row.get(10)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn get_saved_place(&self, id: &str) -> Result<Option<SavedPlaceRow>, String> {
        let id = id.trim();
        if id.is_empty() {
            return Ok(None);
        }
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, name, lat, lng, icon, color, scope, owner_personnel_id, created_at, updated_at, deleted_at
                 FROM saved_places WHERE id = ?1 LIMIT 1",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query(params![id]).map_err(|e| e.to_string())?;
        if let Some(row) = rows.next().map_err(|e| e.to_string())? {
            Ok(Some(SavedPlaceRow {
                id: row.get(0).map_err(|e| e.to_string())?,
                name: row.get(1).map_err(|e| e.to_string())?,
                lat: row.get(2).map_err(|e| e.to_string())?,
                lng: row.get(3).map_err(|e| e.to_string())?,
                icon: row.get(4).map_err(|e| e.to_string())?,
                color: row.get(5).map_err(|e| e.to_string())?,
                scope: row.get(6).map_err(|e| e.to_string())?,
                owner_personnel_id: row.get(7).map_err(|e| e.to_string())?,
                created_at: row.get(8).map_err(|e| e.to_string())?,
                updated_at: row.get(9).map_err(|e| e.to_string())?,
                deleted_at: row.get(10).map_err(|e| e.to_string())?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn get_max_saved_places_updated_at(&self) -> Result<Option<String>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT MAX(updated_at) FROM saved_places WHERE scope = 'shared' AND deleted_at IS NULL",
            [],
            |row| row.get::<_, Option<String>>(0),
        )
        .map_err(|e| e.to_string())
    }

    pub fn list_shared_saved_places_since_updated(
        &self,
        since_updated: &str,
        limit: usize,
    ) -> Result<Vec<SavedPlaceRow>, String> {
        let cap = limit.clamp(1, 500);
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, name, lat, lng, icon, color, scope, owner_personnel_id, created_at, updated_at, deleted_at
                 FROM saved_places
                 WHERE scope = 'shared' AND deleted_at IS NULL AND updated_at > ?1
                 ORDER BY updated_at ASC
                 LIMIT ?2",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![since_updated, cap as i64], |row| {
                Ok(SavedPlaceRow {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    lat: row.get(2)?,
                    lng: row.get(3)?,
                    icon: row.get(4)?,
                    color: row.get(5)?,
                    scope: row.get(6)?,
                    owner_personnel_id: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                    deleted_at: row.get(10)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn upsert_saved_place_if_newer(&self, row: &SavedPlaceRow) -> Result<bool, String> {
        if row.scope != "shared" {
            return Ok(false);
        }
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let n = conn
            .execute(
                "INSERT INTO saved_places (id, name, lat, lng, icon, color, scope, owner_personnel_id, created_at, updated_at, deleted_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                 ON CONFLICT(id) DO UPDATE SET
                   name = excluded.name,
                   lat = excluded.lat,
                   lng = excluded.lng,
                   icon = excluded.icon,
                   color = excluded.color,
                   scope = excluded.scope,
                   owner_personnel_id = excluded.owner_personnel_id,
                   created_at = excluded.created_at,
                   updated_at = excluded.updated_at,
                   deleted_at = excluded.deleted_at
                 WHERE excluded.updated_at > saved_places.updated_at",
                params![
                    row.id,
                    row.name,
                    row.lat,
                    row.lng,
                    row.icon,
                    row.color,
                    row.scope,
                    row.owner_personnel_id,
                    row.created_at,
                    row.updated_at,
                    row.deleted_at,
                ],
            )
            .map_err(|e| e.to_string())?;
        Ok(n > 0)
    }

    pub fn delete_saved_place(&self, id: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM saved_places WHERE id = ?1", params![id.trim()])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn insert_route_blockage(&self, row: &RouteBlockageRow) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO route_blockages (id, geometry_json, reason, severity, modes_affected, starts_at, expires_at, status, proposed_by_personnel_id, approved_by_personnel_id, approved_at, notes, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                row.id,
                row.geometry_json,
                row.reason,
                row.severity,
                row.modes_affected,
                row.starts_at,
                row.expires_at,
                row.status,
                row.proposed_by_personnel_id,
                row.approved_by_personnel_id,
                row.approved_at,
                row.notes,
                row.created_at,
                row.updated_at,
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn update_route_blockage(&self, row: &RouteBlockageRow) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let n = conn
            .execute(
                "UPDATE route_blockages SET geometry_json=?2, reason=?3, severity=?4, modes_affected=?5, starts_at=?6, expires_at=?7, status=?8, approved_by_personnel_id=?9, approved_at=?10, notes=?11, updated_at=?12
                 WHERE id=?1",
                params![
                    row.id,
                    row.geometry_json,
                    row.reason,
                    row.severity,
                    row.modes_affected,
                    row.starts_at,
                    row.expires_at,
                    row.status,
                    row.approved_by_personnel_id,
                    row.approved_at,
                    row.notes,
                    row.updated_at,
                ],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("Blockage not found.".to_string());
        }
        Ok(())
    }

    pub fn list_route_blockages(
        &self,
        include_rejected: bool,
    ) -> Result<Vec<RouteBlockageRow>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let sql = if include_rejected {
            "SELECT id, geometry_json, reason, severity, modes_affected, starts_at, expires_at, status, proposed_by_personnel_id, approved_by_personnel_id, approved_at, notes, created_at, updated_at
             FROM route_blockages ORDER BY updated_at DESC"
        } else {
            "SELECT id, geometry_json, reason, severity, modes_affected, starts_at, expires_at, status, proposed_by_personnel_id, approved_by_personnel_id, approved_at, notes, created_at, updated_at
             FROM route_blockages WHERE status != 'rejected' ORDER BY updated_at DESC"
        };
        let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok(RouteBlockageRow {
                    id: row.get(0)?,
                    geometry_json: row.get(1)?,
                    reason: row.get(2)?,
                    severity: row.get(3)?,
                    modes_affected: row.get(4)?,
                    starts_at: row.get(5)?,
                    expires_at: row.get(6)?,
                    status: row.get(7)?,
                    proposed_by_personnel_id: row.get(8)?,
                    approved_by_personnel_id: row.get(9)?,
                    approved_at: row.get(10)?,
                    notes: row.get(11)?,
                    created_at: row.get(12)?,
                    updated_at: row.get(13)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    /// Active + pending blockages for routing `avoid` lists.
    pub fn list_route_blockages_routing(
        &self,
    ) -> Result<Vec<RouteBlockageRow>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, geometry_json, reason, severity, modes_affected, starts_at, expires_at, status, proposed_by_personnel_id, approved_by_personnel_id, approved_at, notes, created_at, updated_at
                 FROM route_blockages
                 WHERE status = 'active' OR status = 'pending'
                 ORDER BY updated_at DESC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok(RouteBlockageRow {
                    id: row.get(0)?,
                    geometry_json: row.get(1)?,
                    reason: row.get(2)?,
                    severity: row.get(3)?,
                    modes_affected: row.get(4)?,
                    starts_at: row.get(5)?,
                    expires_at: row.get(6)?,
                    status: row.get(7)?,
                    proposed_by_personnel_id: row.get(8)?,
                    approved_by_personnel_id: row.get(9)?,
                    approved_at: row.get(10)?,
                    notes: row.get(11)?,
                    created_at: row.get(12)?,
                    updated_at: row.get(13)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn get_max_route_blockages_updated_at(&self) -> Result<Option<String>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT MAX(updated_at) FROM route_blockages WHERE status IN ('pending','active')",
            [],
            |row| row.get::<_, Option<String>>(0),
        )
        .map_err(|e| e.to_string())
    }

    pub fn list_route_blockages_since_updated(
        &self,
        since: &str,
        limit: usize,
    ) -> Result<Vec<RouteBlockageRow>, String> {
        let cap = limit.clamp(1, 500);
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, geometry_json, reason, severity, modes_affected, starts_at, expires_at, status, proposed_by_personnel_id, approved_by_personnel_id, approved_at, notes, created_at, updated_at
                 FROM route_blockages
                 WHERE status IN ('pending','active') AND updated_at > ?1
                 ORDER BY updated_at ASC
                 LIMIT ?2",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![since, cap as i64], |row| {
                Ok(RouteBlockageRow {
                    id: row.get(0)?,
                    geometry_json: row.get(1)?,
                    reason: row.get(2)?,
                    severity: row.get(3)?,
                    modes_affected: row.get(4)?,
                    starts_at: row.get(5)?,
                    expires_at: row.get(6)?,
                    status: row.get(7)?,
                    proposed_by_personnel_id: row.get(8)?,
                    approved_by_personnel_id: row.get(9)?,
                    approved_at: row.get(10)?,
                    notes: row.get(11)?,
                    created_at: row.get(12)?,
                    updated_at: row.get(13)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn upsert_route_blockage_if_newer(&self, row: &RouteBlockageRow) -> Result<bool, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let n = conn
            .execute(
                "INSERT INTO route_blockages (id, geometry_json, reason, severity, modes_affected, starts_at, expires_at, status, proposed_by_personnel_id, approved_by_personnel_id, approved_at, notes, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
                 ON CONFLICT(id) DO UPDATE SET
                   geometry_json = excluded.geometry_json,
                   reason = excluded.reason,
                   severity = excluded.severity,
                   modes_affected = excluded.modes_affected,
                   starts_at = excluded.starts_at,
                   expires_at = excluded.expires_at,
                   status = excluded.status,
                   proposed_by_personnel_id = excluded.proposed_by_personnel_id,
                   approved_by_personnel_id = excluded.approved_by_personnel_id,
                   approved_at = excluded.approved_at,
                   notes = excluded.notes,
                   created_at = excluded.created_at,
                   updated_at = excluded.updated_at
                 WHERE excluded.updated_at > route_blockages.updated_at",
                params![
                    row.id,
                    row.geometry_json,
                    row.reason,
                    row.severity,
                    row.modes_affected,
                    row.starts_at,
                    row.expires_at,
                    row.status,
                    row.proposed_by_personnel_id,
                    row.approved_by_personnel_id,
                    row.approved_at,
                    row.notes,
                    row.created_at,
                    row.updated_at,
                ],
            )
            .map_err(|e| e.to_string())?;
        Ok(n > 0)
    }

    pub fn get_route_blockage(&self, id: &str) -> Result<Option<RouteBlockageRow>, String> {
        let id = id.trim();
        if id.is_empty() {
            return Ok(None);
        }
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, geometry_json, reason, severity, modes_affected, starts_at, expires_at, status, proposed_by_personnel_id, approved_by_personnel_id, approved_at, notes, created_at, updated_at
                 FROM route_blockages WHERE id = ?1",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query(params![id]).map_err(|e| e.to_string())?;
        if let Some(row) = rows.next().map_err(|e| e.to_string())? {
            Ok(Some(RouteBlockageRow {
                id: row.get(0).map_err(|e| e.to_string())?,
                geometry_json: row.get(1).map_err(|e| e.to_string())?,
                reason: row.get(2).map_err(|e| e.to_string())?,
                severity: row.get(3).map_err(|e| e.to_string())?,
                modes_affected: row.get(4).map_err(|e| e.to_string())?,
                starts_at: row.get(5).map_err(|e| e.to_string())?,
                expires_at: row.get(6).map_err(|e| e.to_string())?,
                status: row.get(7).map_err(|e| e.to_string())?,
                proposed_by_personnel_id: row.get(8).map_err(|e| e.to_string())?,
                approved_by_personnel_id: row.get(9).map_err(|e| e.to_string())?,
                approved_at: row.get(10).map_err(|e| e.to_string())?,
                notes: row.get(11).map_err(|e| e.to_string())?,
                created_at: row.get(12).map_err(|e| e.to_string())?,
                updated_at: row.get(13).map_err(|e| e.to_string())?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn list_territories(&self) -> Result<Vec<TerritoryRow>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, name, status, color, geometry_json, created_at, updated_at, created_by_personnel_id, deleted_at
                 FROM territories
                 WHERE deleted_at IS NULL
                 ORDER BY name COLLATE NOCASE ASC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok(TerritoryRow {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    status: row.get(2)?,
                    color: row.get(3)?,
                    geometry_json: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                    created_by_personnel_id: row.get(7)?,
                    deleted_at: row.get(8)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn list_territories_by_status(&self, status: &str) -> Result<Vec<TerritoryRow>, String> {
        let st = status.trim().to_lowercase();
        if !["friendly", "enemy", "disputed", "unexplored"].contains(&st.as_str()) {
            return Err("Invalid territory status.".to_string());
        }
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, name, status, color, geometry_json, created_at, updated_at, created_by_personnel_id, deleted_at
                 FROM territories
                 WHERE deleted_at IS NULL AND status = ?1
                 ORDER BY updated_at DESC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![st], |row| {
                Ok(TerritoryRow {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    status: row.get(2)?,
                    color: row.get(3)?,
                    geometry_json: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                    created_by_personnel_id: row.get(7)?,
                    deleted_at: row.get(8)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn get_territory_by_name_ci(&self, name: &str) -> Result<Option<TerritoryRow>, String> {
        let n = name.trim();
        if n.is_empty() {
            return Ok(None);
        }
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, name, status, color, geometry_json, created_at, updated_at, created_by_personnel_id, deleted_at
                 FROM territories
                 WHERE deleted_at IS NULL AND LOWER(name) = LOWER(?1)
                 LIMIT 1",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query(params![n]).map_err(|e| e.to_string())?;
        if let Some(row) = rows.next().map_err(|e| e.to_string())? {
            Ok(Some(TerritoryRow {
                id: row.get(0).map_err(|e| e.to_string())?,
                name: row.get(1).map_err(|e| e.to_string())?,
                status: row.get(2).map_err(|e| e.to_string())?,
                color: row.get(3).map_err(|e| e.to_string())?,
                geometry_json: row.get(4).map_err(|e| e.to_string())?,
                created_at: row.get(5).map_err(|e| e.to_string())?,
                updated_at: row.get(6).map_err(|e| e.to_string())?,
                created_by_personnel_id: row.get(7).map_err(|e| e.to_string())?,
                deleted_at: row.get(8).map_err(|e| e.to_string())?,
            }))
        } else {
            Ok(None)
        }
    }

    fn validate_polygon_geometry_json(g: &str) -> Result<(), String> {
        let v: serde_json::Value =
            serde_json::from_str(g).map_err(|e| format!("geometry_json must be valid JSON: {e}"))?;
        let t = v
            .get("type")
            .and_then(|x| x.as_str())
            .unwrap_or("");
        if t != "Polygon" {
            return Err("geometry must be a GeoJSON Polygon object.".to_string());
        }
        if !v.get("coordinates").map(|c| c.is_array()).unwrap_or(false) {
            return Err("Polygon must include coordinates array.".to_string());
        }
        Ok(())
    }

    pub fn insert_territory(&self, row: &TerritoryRow) -> Result<(), String> {
        Self::validate_polygon_geometry_json(&row.geometry_json)?;
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO territories (id, name, status, color, geometry_json, created_at, updated_at, created_by_personnel_id, deleted_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                row.id,
                row.name,
                row.status,
                row.color,
                row.geometry_json,
                row.created_at,
                row.updated_at,
                row.created_by_personnel_id,
                row.deleted_at
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn update_territory_row(&self, row: &TerritoryRow) -> Result<(), String> {
        Self::validate_polygon_geometry_json(&row.geometry_json)?;
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let n = conn
            .execute(
                "UPDATE territories SET name=?2, status=?3, color=?4, geometry_json=?5, updated_at=?6, deleted_at=?7
                 WHERE id = ?1 AND deleted_at IS NULL",
                params![
                    row.id,
                    row.name,
                    row.status,
                    row.color,
                    row.geometry_json,
                    row.updated_at,
                    row.deleted_at
                ],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("Territory not found or already deleted.".to_string());
        }
        Ok(())
    }

    /// Updates status (and `updated_at`) for a non-deleted territory. Used by map assistant tool.
    pub fn update_territory_status_by_id(
        &self,
        id: &str,
        new_status: &str,
        now_rfc3339: &str,
    ) -> Result<TerritoryRow, String> {
        let st = new_status.trim().to_lowercase();
        if !["friendly", "enemy", "disputed", "unexplored"].contains(&st.as_str()) {
            return Err("Invalid status.".to_string());
        }
        let id = id.trim();
        if id.is_empty() {
            return Err("id is required.".to_string());
        }
        let row = self
            .get_territory(id)?
            .ok_or_else(|| "Territory not found.".to_string())?;
        let mut updated = row;
        updated.status = st;
        updated.updated_at = now_rfc3339.to_string();
        {
            let conn = self.conn.lock().map_err(|e| e.to_string())?;
            conn.execute(
                "UPDATE territories SET status=?2, updated_at=?3 WHERE id=?1 AND deleted_at IS NULL",
                params![updated.id, updated.status, updated.updated_at],
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(updated)
    }

    pub fn get_territory(&self, id: &str) -> Result<Option<TerritoryRow>, String> {
        let id = id.trim();
        if id.is_empty() {
            return Ok(None);
        }
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, name, status, color, geometry_json, created_at, updated_at, created_by_personnel_id, deleted_at
                 FROM territories WHERE id = ?1",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query(params![id]).map_err(|e| e.to_string())?;
        if let Some(row) = rows.next().map_err(|e| e.to_string())? {
            Ok(Some(TerritoryRow {
                id: row.get(0).map_err(|e| e.to_string())?,
                name: row.get(1).map_err(|e| e.to_string())?,
                status: row.get(2).map_err(|e| e.to_string())?,
                color: row.get(3).map_err(|e| e.to_string())?,
                geometry_json: row.get(4).map_err(|e| e.to_string())?,
                created_at: row.get(5).map_err(|e| e.to_string())?,
                updated_at: row.get(6).map_err(|e| e.to_string())?,
                created_by_personnel_id: row.get(7).map_err(|e| e.to_string())?,
                deleted_at: row.get(8).map_err(|e| e.to_string())?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn soft_delete_territory(&self, id: &str, now_rfc3339: &str) -> Result<(), String> {
        let id = id.trim();
        if id.is_empty() {
            return Err("id is required.".to_string());
        }
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let n = conn
            .execute(
                "UPDATE territories SET deleted_at=?2, updated_at=?2 WHERE id=?1 AND deleted_at IS NULL",
                params![id, now_rfc3339],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("Territory not found or already deleted.".to_string());
        }
        Ok(())
    }

    pub fn insert_ai_staged_change(&self, row: &AiStagedChangeRow) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO ai_staged_changes (id, entity_type, action, target_entity_id, payload_json, diff_summary, source, proposed_by_personnel_id, status, reviewed_by_personnel_id, reviewed_at, review_notes, auto_approve_rule_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                row.id,
                row.entity_type,
                row.action,
                row.target_entity_id,
                row.payload_json,
                row.diff_summary,
                row.source,
                row.proposed_by_personnel_id,
                row.status,
                row.reviewed_by_personnel_id,
                row.reviewed_at,
                row.review_notes,
                row.auto_approve_rule_id,
                row.created_at,
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn update_ai_staged_status(
        &self,
        id: &str,
        status: &str,
        reviewed_by: Option<&str>,
        reviewed_at: Option<&str>,
        review_notes: Option<&str>,
        auto_rule: Option<&str>,
    ) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE ai_staged_changes SET status=?2, reviewed_by_personnel_id=?3, reviewed_at=?4, review_notes=?5, auto_approve_rule_id=COALESCE(?6, auto_approve_rule_id) WHERE id=?1",
            params![id, status, reviewed_by, reviewed_at, review_notes, auto_rule],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn get_ai_staged_change(&self, id: &str) -> Result<Option<AiStagedChangeRow>, String> {
        let id = id.trim();
        if id.is_empty() {
            return Ok(None);
        }
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, entity_type, action, target_entity_id, payload_json, diff_summary, source, proposed_by_personnel_id, status, reviewed_by_personnel_id, reviewed_at, review_notes, auto_approve_rule_id, created_at
                 FROM ai_staged_changes WHERE id = ?1",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query(params![id]).map_err(|e| e.to_string())?;
        if let Some(row) = rows.next().map_err(|e| e.to_string())? {
            Ok(Some(AiStagedChangeRow {
                id: row.get(0).map_err(|e| e.to_string())?,
                entity_type: row.get(1).map_err(|e| e.to_string())?,
                action: row.get(2).map_err(|e| e.to_string())?,
                target_entity_id: row.get(3).map_err(|e| e.to_string())?,
                payload_json: row.get(4).map_err(|e| e.to_string())?,
                diff_summary: row.get(5).map_err(|e| e.to_string())?,
                source: row.get(6).map_err(|e| e.to_string())?,
                proposed_by_personnel_id: row.get(7).map_err(|e| e.to_string())?,
                status: row.get(8).map_err(|e| e.to_string())?,
                reviewed_by_personnel_id: row.get(9).map_err(|e| e.to_string())?,
                reviewed_at: row.get(10).map_err(|e| e.to_string())?,
                review_notes: row.get(11).map_err(|e| e.to_string())?,
                auto_approve_rule_id: row.get(12).map_err(|e| e.to_string())?,
                created_at: row.get(13).map_err(|e| e.to_string())?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn list_ai_staged_pending(&self) -> Result<Vec<AiStagedChangeRow>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, entity_type, action, target_entity_id, payload_json, diff_summary, source, proposed_by_personnel_id, status, reviewed_by_personnel_id, reviewed_at, review_notes, auto_approve_rule_id, created_at
                 FROM ai_staged_changes
                 WHERE status = 'pending'
                 ORDER BY created_at ASC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok(AiStagedChangeRow {
                    id: row.get(0)?,
                    entity_type: row.get(1)?,
                    action: row.get(2)?,
                    target_entity_id: row.get(3)?,
                    payload_json: row.get(4)?,
                    diff_summary: row.get(5)?,
                    source: row.get(6)?,
                    proposed_by_personnel_id: row.get(7)?,
                    status: row.get(8)?,
                    reviewed_by_personnel_id: row.get(9)?,
                    reviewed_at: row.get(10)?,
                    review_notes: row.get(11)?,
                    auto_approve_rule_id: row.get(12)?,
                    created_at: row.get(13)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn list_ai_staged_for_user(
        &self,
        proposer_id: &str,
    ) -> Result<Vec<AiStagedChangeRow>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, entity_type, action, target_entity_id, payload_json, diff_summary, source, proposed_by_personnel_id, status, reviewed_by_personnel_id, reviewed_at, review_notes, auto_approve_rule_id, created_at
                 FROM ai_staged_changes
                 WHERE proposed_by_personnel_id = ?1 AND status = 'pending'
                 ORDER BY created_at ASC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![proposer_id], |row| {
                Ok(AiStagedChangeRow {
                    id: row.get(0)?,
                    entity_type: row.get(1)?,
                    action: row.get(2)?,
                    target_entity_id: row.get(3)?,
                    payload_json: row.get(4)?,
                    diff_summary: row.get(5)?,
                    source: row.get(6)?,
                    proposed_by_personnel_id: row.get(7)?,
                    status: row.get(8)?,
                    reviewed_by_personnel_id: row.get(9)?,
                    reviewed_at: row.get(10)?,
                    review_notes: row.get(11)?,
                    auto_approve_rule_id: row.get(12)?,
                    created_at: row.get(13)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn list_ai_autoapprove_rules(&self) -> Result<Vec<AiAutoapproveRuleRow>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, entity_type, action, predicate_json, enabled, created_by_personnel_id, created_at
                 FROM ai_autoapprove_rules ORDER BY created_at DESC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok(AiAutoapproveRuleRow {
                    id: row.get(0)?,
                    entity_type: row.get(1)?,
                    action: row.get(2)?,
                    predicate_json: row.get(3)?,
                    enabled: row.get::<_, i64>(4)? != 0,
                    created_by_personnel_id: row.get(5)?,
                    created_at: row.get(6)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn insert_ai_autoapprove_rule(&self, row: &AiAutoapproveRuleRow) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let en: i64 = if row.enabled { 1 } else { 0 };
        conn.execute(
            "INSERT INTO ai_autoapprove_rules (id, entity_type, action, predicate_json, enabled, created_by_personnel_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                row.id,
                row.entity_type,
                row.action,
                row.predicate_json,
                en,
                row.created_by_personnel_id,
                row.created_at
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn delete_ai_autoapprove_rule(&self, id: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "DELETE FROM ai_autoapprove_rules WHERE id = ?1",
            params![id.trim()],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn list_routing_regions(&self) -> Result<Vec<RoutingRegionRow>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, name, bbox_json, source_pbf_path, tiles_path, build_status, modes_built, size_bytes, created_at, geocode_built_at
                 FROM routing_regions ORDER BY name COLLATE NOCASE",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok(RoutingRegionRow {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    bbox_json: row.get(2)?,
                    source_pbf_path: row.get(3)?,
                    tiles_path: row.get(4)?,
                    build_status: row.get(5)?,
                    modes_built: row.get(6)?,
                    size_bytes: row.get(7)?,
                    created_at: row.get(8)?,
                    geocode_built_at: row.get(9)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn get_routing_region(&self, id: &str) -> Result<Option<RoutingRegionRow>, String> {
        let id = id.trim();
        if id.is_empty() {
            return Ok(None);
        }
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, name, bbox_json, source_pbf_path, tiles_path, build_status, modes_built, size_bytes, created_at, geocode_built_at
                 FROM routing_regions WHERE id = ?1",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query(params![id]).map_err(|e| e.to_string())?;
        if let Some(row) = rows.next().map_err(|e| e.to_string())? {
            Ok(Some(RoutingRegionRow {
                id: row.get(0).map_err(|e| e.to_string())?,
                name: row.get(1).map_err(|e| e.to_string())?,
                bbox_json: row.get(2).map_err(|e| e.to_string())?,
                source_pbf_path: row.get(3).map_err(|e| e.to_string())?,
                tiles_path: row.get(4).map_err(|e| e.to_string())?,
                build_status: row.get(5).map_err(|e| e.to_string())?,
                modes_built: row.get(6).map_err(|e| e.to_string())?,
                size_bytes: row.get(7).map_err(|e| e.to_string())?,
                created_at: row.get(8).map_err(|e| e.to_string())?,
                geocode_built_at: row.get(9).map_err(|e| e.to_string())?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn upsert_routing_region(&self, row: &RoutingRegionRow) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO routing_regions (id, name, bbox_json, source_pbf_path, tiles_path, build_status, modes_built, size_bytes, created_at, geocode_built_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET
               name = excluded.name,
               bbox_json = excluded.bbox_json,
               source_pbf_path = excluded.source_pbf_path,
               tiles_path = excluded.tiles_path,
               build_status = excluded.build_status,
               modes_built = excluded.modes_built,
               size_bytes = excluded.size_bytes,
               geocode_built_at = COALESCE(excluded.geocode_built_at, geocode_built_at)",
            params![
                row.id,
                row.name,
                row.bbox_json,
                row.source_pbf_path,
                row.tiles_path,
                row.build_status,
                row.modes_built,
                row.size_bytes,
                row.created_at,
                row.geocode_built_at,
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn delete_routing_region(&self, id: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM routing_regions WHERE id = ?1", params![id.trim()])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Remove all geocoded features for a region (rebuild from PBF).
    pub fn geocode_delete_for_region(&self, region_id: &str) -> Result<usize, String> {
        let rid = region_id.trim();
        if rid.is_empty() {
            return Ok(0);
        }
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let n = conn
            .execute("DELETE FROM geocode_features WHERE region_id = ?1", params![rid])
            .map_err(|e| e.to_string())?;
        Ok(n)
    }

    pub fn geocode_insert(
        &self,
        id: &str,
        region_id: &str,
        feature_class: &str,
        primary_name: Option<&str>,
        alt_names: Option<&str>,
        address_str: Option<&str>,
        city: Option<&str>,
        state: Option<&str>,
        postal: Option<&str>,
        country: Option<&str>,
        lat: f64,
        lon: f64,
        importance: f64,
    ) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO geocode_features (id, region_id, feature_class, primary_name, alt_names, address_str, city, state, postal, country, lat, lon, importance)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(id) DO UPDATE SET
               region_id = excluded.region_id,
               feature_class = excluded.feature_class,
               primary_name = excluded.primary_name,
               alt_names = excluded.alt_names,
               address_str = excluded.address_str,
               city = excluded.city,
               state = excluded.state,
               postal = excluded.postal,
               country = excluded.country,
               lat = excluded.lat,
               lon = excluded.lon,
               importance = excluded.importance",
            params![
                id,
                region_id,
                feature_class,
                primary_name,
                alt_names,
                address_str,
                city,
                state,
                postal,
                country,
                lat,
                lon,
                importance,
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Full-text search over offline geocode index. `fts_match` must be a valid FTS5 MATCH expression.
    pub fn geocode_search_fts(
        &self,
        fts_match: &str,
        prefer_region_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<GeocodeMatch>, String> {
        let take = limit.clamp(1, 100) as i64;
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let prefer = prefer_region_id.unwrap_or("").trim();
        let mut stmt = conn
            .prepare(
                "SELECT g.id, g.region_id, g.feature_class, g.primary_name, g.address_str, g.city, g.state, g.lat, g.lon, g.importance, bm25(geocode_fts)
                 FROM geocode_fts
                 JOIN geocode_features g ON g.rowid = geocode_fts.rowid
                 WHERE geocode_fts MATCH ?1
                 ORDER BY
                   CASE WHEN ?2 != '' AND g.region_id = ?2 THEN 0 ELSE 1 END,
                   bm25(geocode_fts) ASC,
                   g.importance DESC
                 LIMIT ?3",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![fts_match, prefer, take], |row| {
                let primary: Option<String> = row.get(3)?;
                let address: Option<String> = row.get(4)?;
                let city: Option<String> = row.get(5)?;
                let st: Option<String> = row.get(6)?;
                let label = geocode_label_from_row(primary.as_deref(), address.as_deref(), city.as_deref(), st.as_deref());
                Ok(GeocodeMatch {
                    id: row.get(0)?,
                    region_id: row.get(1)?,
                    feature_class: row.get(2)?,
                    label,
                    lat: row.get(7)?,
                    lon: row.get(8)?,
                    importance: row.get(9)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    /// Nearest indexed features to a point (for reverse geocoding / labeling).
    pub fn geocode_nearest(
        &self,
        lat: f64,
        lon: f64,
        region_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<GeocodeMatch>, String> {
        let take = limit.clamp(1, 50) as i64;
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let rid = region_id.unwrap_or("").trim();
        let mut stmt = if rid.is_empty() {
            conn.prepare(
                "SELECT id, region_id, feature_class, primary_name, address_str, city, state, lat, lon, importance
                 FROM geocode_features
                 ORDER BY ((lat - ?1) * (lat - ?1) + (lon - ?2) * (lon - ?2)) ASC
                 LIMIT ?3",
            )
            .map_err(|e| e.to_string())?
        } else {
            conn.prepare(
                "SELECT id, region_id, feature_class, primary_name, address_str, city, state, lat, lon, importance
                 FROM geocode_features
                 WHERE region_id = ?4
                 ORDER BY ((lat - ?1) * (lat - ?1) + (lon - ?2) * (lon - ?2)) ASC
                 LIMIT ?3",
            )
            .map_err(|e| e.to_string())?
        };
        let rows = if rid.is_empty() {
            stmt.query_map(params![lat, lon, take], map_geocode_row)
        } else {
            stmt.query_map(params![lat, lon, take, rid], map_geocode_row)
        }
        .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn set_routing_region_geocode_built(&self, region_id: &str, rfc3339: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE routing_regions SET geocode_built_at = ?2 WHERE id = ?1",
            params![region_id.trim(), rfc3339],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
}

fn map_geocode_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<GeocodeMatch> {
    let primary: Option<String> = row.get(3)?;
    let address: Option<String> = row.get(4)?;
    let city: Option<String> = row.get(5)?;
    let st: Option<String> = row.get(6)?;
    let label = geocode_label_from_row(primary.as_deref(), address.as_deref(), city.as_deref(), st.as_deref());
    Ok(GeocodeMatch {
        id: row.get(0)?,
        region_id: row.get(1)?,
        feature_class: row.get(2)?,
        label,
        lat: row.get(7)?,
        lon: row.get(8)?,
        importance: row.get(9)?,
    })
}

fn geocode_label_from_row(
    primary: Option<&str>,
    address: Option<&str>,
    city: Option<&str>,
    state: Option<&str>,
) -> String {
    if let Some(a) = address.filter(|s| !s.trim().is_empty()) {
        let mut s = a.to_string();
        if let Some(c) = city.filter(|x| !x.trim().is_empty()) {
            s.push_str(", ");
            s.push_str(c);
        }
        if let Some(st) = state.filter(|x| !x.trim().is_empty()) {
            s.push(' ');
            s.push_str(st);
        }
        return s;
    }
    if let Some(p) = primary.filter(|s| !s.trim().is_empty()) {
        return p.to_string();
    }
    "Unnamed".to_string()
}

// --- Local (in-app) chat sessions: one row per chat, scoped by personnel user_id ---

impl Database {
    /// One-time import from legacy `chats/*.json` into `local_chats`, attributed to the earliest-created personnel row.
    pub fn migrate_local_chats_from_legacy_json(&self, app_data_dir: &Path) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM local_chats", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        if n > 0 {
            return Ok(());
        }
        let default_uid: Option<String> = conn
            .query_row(
                "SELECT id FROM personnel ORDER BY created_at ASC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let Some(owner) = default_uid else {
            return Ok(());
        };
        let chats_dir = crate::athena_paths::AthenaPaths::new(app_data_dir).user_chats_dir();
        if !chats_dir.is_dir() {
            // Dual-read: legacy flat chats/ before migration completes.
            let legacy = app_data_dir.join("chats");
            if legacy.is_dir() {
                // fall through with legacy below
            } else {
                return Ok(());
            }
        }
        let chats_dir = if chats_dir.is_dir() {
            chats_dir
        } else {
            app_data_dir.join("chats")
        };
        let entries = std::fs::read_dir(&chats_dir).map_err(|e| e.to_string())?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let data = match std::fs::read_to_string(&path) {
                Ok(d) => d,
                Err(_) => continue,
            };
            let chat: SavedChat = match serde_json::from_str(&data) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let uid = chat
                .user_id
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .unwrap_or_else(|| owner.clone());
            let messages_json =
                serde_json::to_string(&chat.messages).map_err(|e| e.to_string())?;
            conn.execute(
                "INSERT OR REPLACE INTO local_chats (id, user_id, title, description, messages_json, context_summary, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    chat.id,
                    uid,
                    chat.title,
                    chat.description,
                    messages_json,
                    chat.context_summary,
                    chat.created_at,
                    chat.updated_at,
                ],
            )
            .map_err(|e| e.to_string())?;
        }
        drop(conn);
        let legacy = app_data_dir.join(format!(
            "chats__legacy_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        ));
        if let Err(e) = std::fs::rename(&chats_dir, &legacy) {
            eprintln!("[athena] local_chats migration: could not rename chats dir to {:?}: {}", legacy, e);
        }
        Ok(())
    }

    pub fn local_chat_list_summaries(&self, user_id: &str) -> Result<Vec<ChatSummary>, String> {
        let uid = user_id.trim();
        if uid.is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, title, description, updated_at FROM local_chats WHERE user_id = ?1 ORDER BY updated_at DESC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![uid], |row| {
                Ok(ChatSummary {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    description: row.get(2)?,
                    updated_at: row.get(3)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn local_chat_get(&self, chat_id: &str, user_id: &str) -> Result<Option<SavedChat>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, user_id, title, description, messages_json, context_summary, created_at, updated_at
                 FROM local_chats WHERE id = ?1 AND user_id = ?2 LIMIT 1",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query(params![chat_id, user_id])
            .map_err(|e| e.to_string())?;
        let row = match rows.next().map_err(|e| e.to_string())? {
            Some(r) => r,
            None => return Ok(None),
        };
        let mj: String = row.get(4).map_err(|e| e.to_string())?;
        let messages: Vec<SavedMessage> =
            serde_json::from_str(mj.as_str()).map_err(|e| e.to_string())?;
        Ok(Some(SavedChat {
            id: row.get(0).map_err(|e| e.to_string())?,
            user_id: Some(row.get::<_, String>(1).map_err(|e| e.to_string())?),
            title: row.get(2).map_err(|e| e.to_string())?,
            description: row.get(3).map_err(|e| e.to_string())?,
            messages,
            context_summary: row.get(5).map_err(|e| e.to_string())?,
            created_at: row.get(6).map_err(|e| e.to_string())?,
            updated_at: row.get(7).map_err(|e| e.to_string())?,
        }))
    }

    fn local_chat_find_duplicate_id(
        &self,
        user_id: &str,
        messages: &[SavedMessage],
    ) -> Result<Option<String>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare("SELECT id, messages_json FROM local_chats WHERE user_id = ?1")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![user_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?;
        for r in rows {
            let (id, json): (String, String) = r.map_err(|e| e.to_string())?;
            let existing: Vec<SavedMessage> =
                serde_json::from_str(&json).unwrap_or_default();
            if existing.len() == messages.len()
                && existing
                    .iter()
                    .zip(messages.iter())
                    .all(|(a, b)| a.role == b.role && a.content == b.content)
            {
                return Ok(Some(id));
            }
        }
        Ok(None)
    }

    pub fn local_chat_save(
        &self,
        owner_user_id: &str,
        id: Option<String>,
        title: String,
        messages: Vec<SavedMessage>,
        description: Option<String>,
    ) -> Result<String, String> {
        let owner = owner_user_id.trim();
        if owner.is_empty() {
            return Err("user_id is required.".to_string());
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let messages_json = serde_json::to_string(&messages).map_err(|e| e.to_string())?;

        let mut resolved_id = id;
        if resolved_id.is_none() {
            if let Some(dup) = self.local_chat_find_duplicate_id(owner, &messages)? {
                resolved_id = Some(dup);
            }
        }

        let (chat_id, created_at, existing_summary) = if let Some(ref existing_id) = resolved_id {
            let row = self.local_chat_get(existing_id, owner)?;
            if row.is_none() {
                return Err("Chat not found.".to_string());
            }
            let c = row.unwrap();
            (
                existing_id.clone(),
                c.created_at,
                c.context_summary,
            )
        } else {
            let new_id = format!("chat_{}", now);
            (new_id, now, None)
        };

        let title = if title.trim().is_empty() {
            "New chat".to_string()
        } else {
            title
        };

        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO local_chats (id, user_id, title, description, messages_json, context_summary, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET
               user_id = excluded.user_id,
               title = excluded.title,
               description = excluded.description,
               messages_json = excluded.messages_json,
               context_summary = excluded.context_summary,
               updated_at = excluded.updated_at",
            params![
                chat_id,
                owner,
                title,
                description,
                messages_json,
                existing_summary,
                created_at,
                now,
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(chat_id)
    }

    pub fn local_chat_delete(&self, owner_user_id: &str, chat_id: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let n = conn
            .execute(
                "DELETE FROM local_chats WHERE id = ?1 AND user_id = ?2",
                params![chat_id, owner_user_id],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("Chat not found.".to_string());
        }
        Ok(())
    }

    pub fn local_chat_update_metadata(
        &self,
        owner_user_id: &str,
        chat_id: &str,
        title: Option<String>,
        description: Option<String>,
    ) -> Result<(), String> {
        let row = self
            .local_chat_get(chat_id, owner_user_id)?
            .ok_or_else(|| "Chat not found.".to_string())?;
        let mut new_title = row.title;
        let mut new_desc = row.description;
        if let Some(t) = title {
            new_title = t;
        }
        if let Some(d) = description {
            new_desc = Some(d);
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let messages_json = serde_json::to_string(&row.messages).map_err(|e| e.to_string())?;
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE local_chats SET title = ?1, description = ?2, updated_at = ?3, messages_json = ?4 WHERE id = ?5 AND user_id = ?6",
            params![
                new_title,
                new_desc,
                now,
                messages_json,
                chat_id,
                owner_user_id,
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn local_chat_update_context_summary(
        &self,
        owner_user_id: &str,
        chat_id: &str,
        last_user_msg: &str,
        last_assistant_msg: &str,
    ) -> Result<(), String> {
        let row = self
            .local_chat_get(chat_id, owner_user_id)?
            .ok_or_else(|| "Chat not found.".to_string())?;
        let combined = format!("{} {}", last_user_msg, last_assistant_msg);
        let words: Vec<&str> = combined
            .split_whitespace()
            .filter(|w| {
                w.len() >= 3 && w.chars().all(|c| c.is_ascii_alphanumeric() || c == '\'')
            })
            .take(50)
            .collect();
        let new_line = words.join(" ");
        let updated = if let Some(ref existing) = row.context_summary {
            let trimmed = existing.trim();
            if trimmed.is_empty() {
                new_line
            } else {
                format!("{} {}", trimmed, new_line)
            }
        } else {
            new_line
        };
        let summary = if updated.len() > 600 {
            updated
                .chars()
                .skip(updated.len() - 600)
                .collect::<String>()
        } else {
            updated
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let messages_json = serde_json::to_string(&row.messages).map_err(|e| e.to_string())?;
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE local_chats SET context_summary = ?1, updated_at = ?2, messages_json = ?3 WHERE id = ?4 AND user_id = ?5",
            params![summary, now, messages_json, chat_id, owner_user_id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::Database;
    use crate::embeddings::EMBEDDING_DIM;
    use tempfile::tempdir;

    /// With sqlite-vec KNN, the globally best-matching row must appear in the top-k set even when its
    /// `id` is large (previously missed by bounded `ORDER BY id LIMIT` + cosine fallback).
    #[test]
    fn semantic_search_knn_includes_global_best_match() {
        let dir = tempdir().expect("temp dir");
        let db_path = dir.path().join("semantic_stress.db");
        let db = Database::new(&db_path).expect("open db");

        let mut query = vec![0.0f32; EMBEDDING_DIM];
        query[0] = 1.0;

        let mut orthogonal = vec![0.0f32; EMBEDDING_DIM];
        // Must clear the RAG min similarity threshold so we get a full top-k set.
        // Keep it distinct from `query` so the exact match row remains the global best.
        orthogonal[0] = 0.70;
        orthogonal[1] = 0.71414286;

        for i in 1..=50_000 {
            let file_path = format!("stress/{i}.txt");
            let vec = if i == 25_001 {
                query.clone()
            } else {
                orthogonal.clone()
            };
            db.upsert_chunk_embedding(&file_path, 0, "chunk", &vec)
                .unwrap_or_else(|e| panic!("upsert {i}: {e}"));
        }

        let results = db
            .search_similar_chunks_with_vec(&query, 50)
            .expect("search_similar_chunks");
        assert_eq!(results.len(), 50);
        assert!(
            results.iter().any(|c| c.id == 25_001),
            "sqlite-vec kNN should retrieve the exact query vector row (id 25_001) in the top set"
        );
    }

    #[test]
    fn chunk_embeddings_has_needs_embedding_column() {
        let dir = tempdir().expect("temp dir");
        let db_path = dir.path().join("needs_emb.db");
        {
            let _db = Database::new(&db_path).expect("open db");
        }
        let conn = rusqlite::Connection::open(&db_path).expect("reopen");
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('chunk_embeddings') WHERE name = 'needs_embedding'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn insert_chunk_embedding_respects_needs_embedding_flag() {
        let dir = tempdir().expect("temp dir");
        let db_path = dir.path().join("needs_emb2.db");
        let id = {
            let db = Database::new(&db_path).expect("open db");
            let v = vec![0.0f32; EMBEDDING_DIM];
            db.insert_chunk_embedding_row(
                "/t.txt",
                0,
                None,
                None,
                None,
                None,
                "hello",
                "hello",
                &v,
                true,
                false,
                None,
            )
            .expect("insert")
        };
        let conn = rusqlite::Connection::open(&db_path).expect("reopen");
        let flag: i64 = conn
            .query_row(
                "SELECT needs_embedding FROM chunk_embeddings WHERE id = ?1",
                [id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(flag, 1);
    }
}
