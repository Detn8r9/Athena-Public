use chrono::Utc;
use rusqlite::backup::Backup;
use rusqlite::ffi::sqlite3_auto_extension;
use rusqlite::OptionalExtension;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sqlite_vec::sqlite3_vec_init;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

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
}

/// One expanded context block for hybrid retrieval + reranking (Phase 3).
#[derive(Debug, Clone, Serialize)]
pub struct HybridSearchHit {
    pub expanded_text: String,
    pub display_title: Option<String>,
    pub page_number: Option<i64>,
    pub file_path: String,
}

/// Reranked child chunks (with parent context) passed to the LLM for library RAG (`hybrid_search`).
pub const HYBRID_SEARCH_TOP_CHUNKS: usize = 15;

#[derive(Debug, Clone)]
struct HybridChunkRow {
    retrieval_text: String,
    chunk_text: String,
    parent_id: Option<i64>,
    file_path: String,
    page_start: Option<i64>,
    display_title: Option<String>,
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
}

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

/// Stored encrypted channel profile (frequency + PSK); never written to `radio_logs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RadioChannel {
    pub id: String,
    pub name: String,
    pub frequency_mhz: f64,
    pub encryption_key: String,
    pub created_at: String,
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
    #[serde(default)]
    pub parent_entry_id: Option<String>,
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

impl Database {
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
        let db = Database {
            conn: Mutex::new(conn),
        };
        db.init_schema()?;
        db.migrate_rag_phase1_schema()?;
        db.migrate_indexed_files_relative_path()?;
        db.migrate_library_chunks_fts()?;
        db.migrate_chunk_embeddings_vec384()?;
        db.migrate_map_schema()?;
        db.migrate_radio_telemetry_schema()?;
        db.migrate_logbook_schema()?;
        Ok(db)
    }

    /// Opens an in-memory database with the same schema as on-disk installs. Used when swapping the
    /// on-disk file during restore so the previous [`Connection`] is fully dropped.
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
        db.migrate_indexed_files_relative_path()?;
        db.migrate_library_chunks_fts()?;
        db.migrate_chunk_embeddings_vec384()?;
        db.migrate_map_schema()?;
        db.migrate_radio_telemetry_schema()?;
        db.migrate_logbook_schema()?;
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
                parent_entry_id TEXT REFERENCES logbook_entries(id)
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
                ingested_at DATETIME NOT NULL
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
                created_at INTEGER NOT NULL
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_mesh_profiles_node_id ON mesh_profiles(node_id);
            CREATE TABLE IF NOT EXISTS mesh_chats (
                id TEXT PRIMARY KEY NOT NULL,
                node_id INTEGER NOT NULL,
                sender_type TEXT NOT NULL CHECK (sender_type IN ('human','ai','remote')),
                message_type TEXT NOT NULL CHECK (message_type IN ('auth','chat','prompt','response')),
                content TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_mesh_chats_node_created ON mesh_chats(node_id, created_at ASC);
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
        let _ = conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_logbook_parent_entry_id ON logbook_entries(parent_entry_id)",
            [],
        );
        Ok(())
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
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
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
        .map_err(|e| e.to_string())?;
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
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let like = format!("{}%", prefix);
        conn.execute(
            "DELETE FROM indexed_files WHERE path = ?1 OR path LIKE ?2",
            params![prefix, like],
        )
        .map_err(|e| e.to_string())?;
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
        sync_to_vec_index: bool,
    ) -> Result<i64, String> {
        if vector.len() != embeddings::EMBEDDING_DIM {
            return Err(format!(
                "vector length {} must be {}",
                vector.len(),
                embeddings::EMBEDDING_DIM
            ));
        }
        let now = Utc::now().timestamp();
        let blob = bytemuck::cast_slice::<f32, u8>(vector).to_vec();
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "
            INSERT INTO chunk_embeddings (file_path, chunk_index, chunk_layer, parent_id, page_start, page_end, chunk_text, retrieval_text, vector, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ON CONFLICT(file_path, chunk_index) DO UPDATE SET
                chunk_layer = excluded.chunk_layer,
                parent_id = excluded.parent_id,
                page_start = excluded.page_start,
                page_end = excluded.page_end,
                chunk_text = excluded.chunk_text,
                retrieval_text = excluded.retrieval_text,
                vector = excluded.vector,
                created_at = excluded.created_at
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
                now
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
            true,
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
                        "SELECT id, file_path, chunk_index, chunk_text, vector FROM chunk_embeddings WHERE id IN ({placeholders}) AND IFNULL(chunk_layer, '') != 'parent'"
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
                        let blob: Vec<u8> = row.get(4).map_err(|e| e.to_string())?;
                        let expected_len = embeddings::EMBEDDING_DIM * std::mem::size_of::<f32>();
                        let vector: Vec<f32> = if blob.len() != expected_len {
                            vec![0.0; embeddings::EMBEDDING_DIM]
                        } else {
                            bytemuck::cast_slice::<u8, f32>(&blob).to_vec()
                        };
                        out.push(ChunkEmbedding {
                            id,
                            file_path,
                            chunk_index,
                            chunk_text,
                            vector,
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
                "SELECT id, file_path, chunk_index, chunk_text, vector FROM chunk_embeddings WHERE IFNULL(chunk_layer, '') != 'parent' ORDER BY id LIMIT ?1",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![candidate_limit as i64], |row| {
                let id: i64 = row.get(0)?;
                let file_path: String = row.get(1)?;
                let chunk_index: i64 = row.get(2)?;
                let chunk_text: String = row.get(3)?;
                let blob: Vec<u8> = row.get(4)?;
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
            "SELECT c.id, TRIM(COALESCE(c.retrieval_text, c.chunk_text, '')), c.chunk_text, c.parent_id, c.file_path, c.page_start, c.page_end, f.display_title \
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
            map.insert(
                id,
                HybridChunkRow {
                    retrieval_text,
                    chunk_text,
                    parent_id,
                    file_path,
                    page_start,
                    display_title,
                },
            );
        }
        Ok(map)
    }

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

    /// BM25 + vector union (top 50 each), cross-encoder rerank, interleaved pools, top-K child chunks with parent + child context for the LLM.
    pub fn hybrid_search(&self, query: &str) -> Result<Vec<HybridSearchHit>, String> {
        const TOP_K: usize = 50;
        const POOL: i64 = 150;
        const MAX_PER_FILE_POOL: usize = 4;
        let winners = HYBRID_SEARCH_TOP_CHUNKS;
        let mut fts_query = query
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.len() > 3)
            .collect::<Vec<_>>()
            .join(" OR ");
        if fts_query.is_empty() {
            fts_query = query
                .split(|c: char| !c.is_alphanumeric())
                .filter(|w| !w.is_empty())
                .collect::<Vec<_>>()
                .join(" OR ");
        }
        let fts_raw = self.fts_top_child_chunk_ids_for_match_string(&fts_query, POOL)?;
        let fts_ids = {
            let conn = self.conn.lock().map_err(|e| e.to_string())?;
            Self::diversify_child_ids_by_file_path(&conn, &fts_raw, TOP_K, MAX_PER_FILE_POOL)?
        };
        let semantic = self
            .search_similar_chunks(query, POOL as usize)
            .unwrap_or_default();
        let sem_ids = Self::diversify_semantic_chunk_ids(&semantic, TOP_K, MAX_PER_FILE_POOL);
        let mut seen = std::collections::HashSet::<i64>::new();
        let mut ordered_ids: Vec<i64> = Vec::new();
        let max_len = fts_ids.len().max(sem_ids.len());
        for i in 0..max_len {
            if i < fts_ids.len() {
                let id = fts_ids[i];
                if seen.insert(id) {
                    ordered_ids.push(id);
                }
            }
            if i < sem_ids.len() {
                let id = sem_ids[i];
                if seen.insert(id) {
                    ordered_ids.push(id);
                }
            }
        }
        if ordered_ids.is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let row_map = Self::load_hybrid_chunk_rows(&conn, &ordered_ids)?;
        let passages: Vec<String> = ordered_ids
            .iter()
            .map(|id| {
                row_map
                    .get(id)
                    .map(|r| r.retrieval_text.clone())
                    .unwrap_or_default()
            })
            .collect();
        drop(conn);
        let scores = crate::rerank::score_pairs_global(query, &passages)?;
        if scores.len() != ordered_ids.len() {
            return Err("reranker score count mismatch".to_string());
        }
        let mut ranked: Vec<(f32, i64)> = scores
            .into_iter()
            .zip(ordered_ids.into_iter())
            .collect();
        ranked.sort_by(|a, b| {
            b.0.partial_cmp(&a.0)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        use std::collections::HashMap;
        // Round-robin across files in rank order so one very strong document cannot occupy every slot
        // after rerank (common when prior chat biased the query toward that file).
        let mut by_file: Vec<(String, Vec<i64>)> = Vec::new();
        let mut file_index: HashMap<String, usize> = HashMap::new();
        for (_score, id) in &ranked {
            let Some(row) = row_map.get(id) else {
                continue;
            };
            let fp = row.file_path.clone();
            if let Some(&idx) = file_index.get(&fp) {
                by_file[idx].1.push(*id);
            } else {
                file_index.insert(fp.clone(), by_file.len());
                by_file.push((fp, vec![*id]));
            }
        }
        let mut top_ids: Vec<i64> = Vec::new();
        let mut round: usize = 0;
        while top_ids.len() < winners {
            let mut any = false;
            for (_, ids) in &by_file {
                if let Some(id) = ids.get(round) {
                    if !top_ids.contains(id) {
                        top_ids.push(*id);
                        any = true;
                    }
                    if top_ids.len() >= winners {
                        break;
                    }
                }
            }
            if !any {
                break;
            }
            round += 1;
        }
        if top_ids.len() < winners {
            for (_score, id) in &ranked {
                if top_ids.len() >= winners {
                    break;
                }
                if top_ids.contains(id) {
                    continue;
                }
                top_ids.push(*id);
            }
        }
        if top_ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut hits = Vec::new();
        let mut seen_expanded: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        for id in top_ids {
            let Some(row) = row_map.get(&id) else {
                continue;
            };
            // Bypass parent expansion: send the raw child chunk text only.
            let expanded_text = row.chunk_text.clone();
            let preview_key = expanded_text
                .chars()
                .take(100)
                .collect::<String>()
                .to_lowercase()
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>();
            if !seen_expanded.insert(preview_key) {
                continue;
            }
            hits.push(HybridSearchHit {
                expanded_text,
                display_title: row.display_title.clone(),
                page_number: row.page_start,
                file_path: row.file_path.clone(),
            });
        }
        for (i, h) in hits.iter().enumerate() {
            let preview: String = h.expanded_text.chars().take(200).collect();
            println!("[athena] Retrieved Chunk {}: {}", i + 1, preview);
        }
        Ok(hits)
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

    pub fn list_logbook_entries(&self) -> Result<Vec<LogbookEntry>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, body, created_at, from_node_id, author_name, author_occupation, parent_entry_id FROM logbook_entries ORDER BY created_at DESC",
            )
            .map_err(|e| e.to_string())?;
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
                    parent_entry_id: row.get(6)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
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
                "SELECT id, body, created_at, from_node_id, author_name, author_occupation, parent_entry_id
                 FROM logbook_entries
                 WHERE created_at > ?1
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
                    parent_entry_id: row.get(6)?,
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
                "SELECT id, body, created_at, from_node_id, author_name, author_occupation, parent_entry_id FROM logbook_entries ORDER BY created_at DESC LIMIT ?1",
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
                    parent_entry_id: row.get(6)?,
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
            .prepare("SELECT id, body, created_at, from_node_id, author_name, author_occupation, parent_entry_id FROM logbook_entries WHERE id = ?1 LIMIT 1")
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
                parent_entry_id: row.get(6).map_err(|e| e.to_string())?,
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
    ) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO logbook_entries (id, body, created_at, from_node_id, author_name, author_occupation, parent_entry_id) VALUES (?1, ?2, ?3, NULL, ?4, ?5, ?6)",
            params![id, body, created_at, author_name, author_occupation, parent_entry_id],
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
        let n = conn
            .execute(
                "INSERT OR IGNORE INTO logbook_entries (id, body, created_at, from_node_id, author_name, author_occupation, parent_entry_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    entry.id.trim(),
                    entry.body.as_str(),
                    entry.created_at,
                    from_raw,
                    entry.author_name.as_str(),
                    entry.author_occupation.as_str(),
                    entry.parent_entry_id.as_deref(),
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
            "INSERT INTO mesh_profiles (id, node_id, username, counter_sign, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(node_id) DO UPDATE SET
               username = excluded.username,
               counter_sign = excluded.counter_sign",
            params![id.clone(), node_id, username, counter_sign, now],
        )
        .map_err(|e| e.to_string())?;
        drop(conn);
        self.mesh_profile_get_by_node(node_id)?.ok_or_else(|| "Profile upsert failed.".to_string())
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
                "SELECT id, node_id, username, counter_sign, created_at FROM mesh_profiles ORDER BY username ASC",
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
                "SELECT id, node_id, username, counter_sign, created_at FROM mesh_profiles WHERE node_id = ?1 LIMIT 1",
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
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO mesh_chats (id, node_id, sender_type, message_type, content, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, node_id, sender_type, message_type, content, created_at],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
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
        orthogonal[1] = 1.0;

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
}
