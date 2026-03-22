use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;

use crate::embeddings;

pub fn is_stopword(w: &str) -> bool {
    matches!(
        w,
        "a" | "an" | "and" | "are" | "as" | "at" | "be" | "but" | "by" | "can" | "could" | "did" | "do" | "does"
            | "for" | "from" | "had" | "has" | "have" | "how" | "i" | "if" | "in" | "into" | "is" | "it" | "its"
            | "let" | "me" | "my" | "of" | "on" | "or" | "our" | "so" | "that" | "the" | "their" | "them" | "then"
            | "there" | "these" | "they" | "this" | "to" | "was" | "we" | "were" | "what" | "when" | "where"
            | "which" | "who" | "why" | "will" | "with" | "you" | "your"
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

impl Database {
    pub fn new(db_path: &Path) -> Result<Self, String> {
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
        Ok(db)
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
            ",
        )
        .map_err(|e| e.to_string())?;
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
            params![path, format!("{}%", dir_prefix), format!("{}%", derived_prefix)],
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
        conn.execute("DELETE FROM indexed_files WHERE path = ?1 OR path LIKE ?2", params![prefix, like])
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
            let where_ors: Vec<String> = (0..patterns.len()).map(|i| format!("text_content LIKE ?{}", i + 1)).collect();
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
    pub fn get_indexed_files_by_name_mentioned(&self, query: &str) -> Result<Vec<IndexedFile>, String> {
        let all = self.get_all()?;
        let query_lower = query.to_lowercase();
        let out: Vec<IndexedFile> = all
            .into_iter()
            .filter(|f| {
                f.text_content.as_deref().map_or(false, |t| !t.trim().is_empty())
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
            .query_row("SELECT COUNT(*) FROM indexed_files WHERE file_type = 'text'", [], |row| row.get(0))
            .map_err(|e| e.to_string())?;
        let image_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM indexed_files WHERE file_type = 'image'", [], |row| row.get(0))
            .map_err(|e| e.to_string())?;
        let video_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM indexed_files WHERE file_type = 'video'", [], |row| row.get(0))
            .map_err(|e| e.to_string())?;
        let map_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM indexed_files WHERE file_type = 'map'", [], |row| row.get(0))
            .map_err(|e| e.to_string())?;
        let zim_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM indexed_files WHERE file_type = 'zim'", [], |row| row.get(0))
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
            .query_row("SELECT COUNT(*) FROM chunk_embeddings", [], |row| row.get(0))
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

    pub fn list_tactical_mesh_node_locations(&self) -> Result<Vec<TacticalMeshNodeLocation>, String> {
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
            params![file_path, chunk_index, chunk_text, blob, now],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn delete_embeddings_for_file(&self, file_path: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM chunk_embeddings WHERE file_path = ?1", params![file_path])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Remove all chunk embeddings whose file_path equals prefix or starts with prefix/ (e.g. when a directory is removed).
    pub fn delete_embeddings_by_path_prefix(&self, prefix: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let like = format!("{}%", prefix);
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
        // Never load the entire `chunk_embeddings` table: score a bounded candidate set, then take top_k.
        let min_candidates: usize = 2_000;
        let per_topk: usize = 500;
        let max_candidates: usize = 25_000;
        let candidate_limit = top_k
            .saturating_mul(per_topk)
            .max(min_candidates)
            .min(max_candidates);

        let conn = self.conn.lock().map_err(|e| e.to_string())?;
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
        Ok(with_score
            .into_iter()
            .take(top_k)
            .map(|(_, c)| c)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::Database;
    use crate::embeddings::EMBEDDING_DIM;
    use tempfile::tempdir;

    /// Phase 3: With 50k rows, `search_similar_chunks` uses `ORDER BY id LIMIT` bounded by 25_000
    /// before cosine scoring. The perfect-match row at id 25_001 must not appear in results when
    /// `top_k=50` (candidate_limit = 50 × 500 = 25_000).
    #[test]
    fn semantic_search_limits_candidate_fetch_to_25000_before_cosine() {
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
            !results.iter().any(|c| c.id == 25_001),
            "id 25_001 has cosine 1.0 with the query but must be outside the 25_000-row candidate window"
        );
    }
}
