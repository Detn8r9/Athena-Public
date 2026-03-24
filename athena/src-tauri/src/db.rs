use chrono::Utc;
use rusqlite::backup::Backup;
use rusqlite::ffi::sqlite3_auto_extension;
use rusqlite::{params, Connection};
use sqlite_vec::sqlite3_vec_init;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use crate::embeddings;

/// Register sqlite-vec once per process before opening any SQLite connection (must match bundled rusqlite).
fn ensure_sqlite_vec_auto_extension() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| unsafe {
        let _ = sqlite3_auto_extension(Some(std::mem::transmute(
            sqlite3_vec_init as *const (),
        )));
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
}

#[derive(Debug, Clone, Serialize)]
pub struct ChunkEmbedding {
    pub id: i64,
    pub file_path: String,
    pub chunk_index: i64,
    pub chunk_text: String,
    pub vector: Vec<f32>,
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

/// Stored encrypted channel profile (frequency + PSK); never written to `radio_logs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RadioChannel {
    pub id: String,
    pub name: String,
    pub frequency_mhz: f64,
    pub encryption_key: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LogbookEntry {
    pub id: String,
    pub body: String,
    pub created_at: i64,
    /// Set when the entry was received from another node over Meshtastic (`None` = created on this device).
    pub from_node_id: Option<u32>,
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
        db.migrate_map_schema()?;
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
        db.migrate_map_schema()?;
        Ok(db)
    }

    /// Copies the live database to `dest` using SQLite's online backup API (WAL-safe).
    pub fn backup_to_path(&self, dest: &Path) -> Result<(), String> {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let src = self.conn.lock().map_err(|e| e.to_string())?;
        let mut dst = Connection::open(dest).map_err(|e| e.to_string())?;
        let backup = Backup::new(&*src, &mut dst).map_err(|e| e.to_string())?;
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
                created_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_indexed_files_path ON indexed_files(path);
            CREATE INDEX IF NOT EXISTS idx_indexed_files_file_type ON indexed_files(file_type);
            CREATE TABLE IF NOT EXISTS chunk_embeddings (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                file_path TEXT NOT NULL,
                chunk_index INTEGER NOT NULL,
                chunk_text TEXT NOT NULL,
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
                from_node_id INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_logbook_created_at ON logbook_entries(created_at DESC);
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
            CREATE TABLE IF NOT EXISTS radio_channels (
                id TEXT PRIMARY KEY NOT NULL,
                name TEXT NOT NULL,
                frequency_mhz REAL NOT NULL,
                encryption_key TEXT NOT NULL,
                created_at DATETIME NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_radio_channels_name ON radio_channels(name);
            ",
        )
        .map_err(|e| e.to_string())?;
        if let Err(e) = conn.execute(
            "CREATE VIRTUAL TABLE IF NOT EXISTS chunk_embeddings_vec USING vec0( embedding float[256] );",
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
            .prepare("SELECT id, vector FROM chunk_embeddings")
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
        let vec_count: i64 = match conn.query_row(
            "SELECT COUNT(*) FROM chunk_embeddings_vec",
            [],
            |r| r.get(0),
        ) {
            Ok(c) => c,
            Err(_) => return Ok(()),
        };
        if chunk_count == vec_count {
            return Ok(());
        }
        drop(conn);
        self.rebuild_chunk_embeddings_vec()
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

    pub fn upsert_file(
        &self,
        path: &str,
        file_type: &str,
        name: &str,
        size: Option<i64>,
        modified_at: Option<i64>,
        text_content: Option<&str>,
        metadata_json: Option<&str>,
    ) -> Result<(), String> {
        let now = Utc::now().timestamp();
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "
            INSERT INTO indexed_files (path, file_type, name, size, modified_at, text_content, metadata_json, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(path) DO UPDATE SET
                file_type = excluded.file_type,
                name = excluded.name,
                size = excluded.size,
                modified_at = excluded.modified_at,
                text_content = excluded.text_content,
                metadata_json = excluded.metadata_json
            ",
            params![path, file_type, name, size, modified_at, text_content, metadata_json, now],
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
                "SELECT id, path, file_type, name, size, modified_at, text_content, metadata_json, created_at
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

    pub fn get_all(&self) -> Result<Vec<IndexedFile>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare("SELECT id, path, file_type, name, size, modified_at, text_content, metadata_json, created_at FROM indexed_files ORDER BY path")
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
        if !keywords.is_empty() {
            let patterns: Vec<String> = keywords.iter().map(|w| format!("%{}%", w)).collect();
            let where_ors: Vec<String> = (0..patterns.len())
                .map(|i| format!("text_content LIKE ?{}", i + 1))
                .collect();
            let score_terms: Vec<String> = (0..patterns.len())
                .map(|i| format!("CASE WHEN text_content LIKE ?{} THEN 1 ELSE 0 END", i + 1))
                .collect();

            let mut sql = String::new();
            sql.push_str(
                "SELECT id, path, file_type, name, size, modified_at, text_content, metadata_json, created_at
                 FROM indexed_files
                 WHERE (file_type = 'text' OR file_type = 'zim' OR file_type = 'learned' OR file_type = 'image')
                   AND text_content IS NOT NULL
                   AND text_content != ''
                   AND (",
            );
            sql.push_str(&where_ors.join(" OR "));
            sql.push_str(")\nORDER BY (");
            sql.push_str(&score_terms.join(" + "));
            sql.push_str(") DESC, created_at DESC\n");
            sql.push_str(&format!("LIMIT ?{}", patterns.len() + 1));

            let mut values: Vec<Box<dyn rusqlite::ToSql>> = Vec::with_capacity(patterns.len() + 1);
            for p in patterns {
                values.push(Box::new(p));
            }
            values.push(Box::new(limit));

            let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(
                    rusqlite::params_from_iter(values.iter().map(|v| &**v)),
                    |row| {
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
                        })
                    },
                )
                .map_err(|e| e.to_string())?;
            for row in rows {
                out.push(row.map_err(|e| e.to_string())?);
            }
            return Ok(out);
        }

        // Fallback: return recent text rows (helps when query is empty or all stopwords).
        let mut stmt = conn
            .prepare(
                "SELECT id, path, file_type, name, size, modified_at, text_content, metadata_json, created_at
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
                    .map_or(false, |t| !t.trim().is_empty())
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
                "SELECT id, path, file_type, name, size, modified_at, text_content, metadata_json, created_at
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

    pub fn get_tactical_point_of_interest_by_id(&self, id: &str) -> Result<Option<TacticalPointOfInterest>, String> {
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
        Ok(rows.next().transpose().map_err(|e| e.to_string())?)
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

    pub fn insert_radio_telemetry(&self, rssi: i64, snr: f64) -> Result<i64, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let ts = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO radio_telemetry (timestamp, rssi, snr) VALUES (?1, ?2, ?3)",
            params![ts, rssi, snr],
        )
        .map_err(|e| e.to_string())?;
        Ok(conn.last_insert_rowid())
    }

    pub fn list_recent_radio_telemetry(&self, limit: usize) -> Result<Vec<RadioTelemetrySample>, String> {
        let cap = limit.clamp(1, 2000);
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut statement = conn
            .prepare(
                "SELECT id, timestamp, rssi, snr
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
                    rssi: row.get(2)?,
                    snr: row.get(3)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn insert_mesh_message_pending(&self, packet_id: &str, message: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO mesh_messages (packet_id, message, status, created_at, updated_at)
             VALUES (?1, ?2, 'pending', ?3, ?3)",
            params![packet_id, message, now],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn mark_mesh_message_delivered(&self, packet_id: &str) -> Result<bool, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let now = Utc::now().to_rfc3339();
        let n = conn
            .execute(
                "UPDATE mesh_messages SET status = 'delivered', updated_at = ?2 WHERE packet_id = ?1 AND status = 'pending'",
                params![packet_id.trim(), now],
            )
            .map_err(|e| e.to_string())?;
        Ok(n > 0)
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
        conn.execute("DELETE FROM radio_channels WHERE id = ?1", params![id.trim()])
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

    pub fn upsert_chunk_embedding(
        &self,
        file_path: &str,
        chunk_index: i64,
        chunk_text: &str,
        vector: &[f32],
    ) -> Result<(), String> {
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
            INSERT INTO chunk_embeddings (file_path, chunk_index, chunk_text, vector, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(file_path, chunk_index) DO UPDATE SET
                chunk_text = excluded.chunk_text,
                vector = excluded.vector,
                created_at = excluded.created_at
            ",
            params![file_path, chunk_index, chunk_text, blob.clone(), now],
        )
        .map_err(|e| e.to_string())?;
        let rowid = conn.last_insert_rowid();
        let _ = conn.execute(
            "INSERT OR REPLACE INTO chunk_embeddings_vec(rowid, embedding) VALUES (?1, ?2)",
            params![rowid, blob],
        );
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
        let k = top_k.max(1).min(500) as i64;
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
                        "SELECT id, file_path, chunk_index, chunk_text, vector FROM chunk_embeddings WHERE id IN ({placeholders})"
                    );
                    let mut stmt2 = conn.prepare(&sql).map_err(|e| e.to_string())?;
                    let params: Vec<&dyn rusqlite::ToSql> = ids
                        .iter()
                        .map(|id| id as &dyn rusqlite::ToSql)
                        .collect();
                    let mut out: Vec<ChunkEmbedding> = Vec::new();
                    let mut q = stmt2.query(rusqlite::params_from_iter(params.iter().copied()))
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
                    let order: std::collections::HashMap<i64, usize> = ids
                        .iter()
                        .enumerate()
                        .map(|(i, id)| (*id, i))
                        .collect();
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
                "SELECT id, file_path, chunk_index, chunk_text, vector FROM chunk_embeddings ORDER BY id LIMIT ?1",
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
        with_score.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        Ok(with_score.into_iter().take(top_k).map(|(_, c)| c).collect())
    }

    pub fn list_logbook_entries(&self) -> Result<Vec<LogbookEntry>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, body, created_at, from_node_id FROM logbook_entries ORDER BY created_at DESC",
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
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
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
                "SELECT id, body, created_at, from_node_id FROM logbook_entries ORDER BY created_at DESC LIMIT ?1",
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
            .prepare("SELECT id, body, created_at, from_node_id FROM logbook_entries WHERE id = ?1 LIMIT 1")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query(params![id]).map_err(|e| e.to_string())?;
        if let Some(row) = rows.next().map_err(|e| e.to_string())? {
            let from_raw: Option<i64> = row.get(3).map_err(|e| e.to_string())?;
            Ok(Some(LogbookEntry {
                id: row.get(0).map_err(|e| e.to_string())?,
                body: row.get(1).map_err(|e| e.to_string())?,
                created_at: row.get(2).map_err(|e| e.to_string())?,
                from_node_id: from_raw.and_then(|n| u32::try_from(n).ok()),
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
    ) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO logbook_entries (id, body, created_at, from_node_id) VALUES (?1, ?2, ?3, NULL)",
            params![id, body, created_at],
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

    pub fn delete_logbook_entry(&self, id: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM logbook_entries WHERE id = ?1", params![id])
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
            .search_similar_chunks(&query, 50)
            .expect("search_similar_chunks");
        assert_eq!(results.len(), 50);
        assert!(
            results.iter().any(|c| c.id == 25_001),
            "sqlite-vec kNN should retrieve the exact query vector row (id 25_001) in the top set"
        );
    }
}
