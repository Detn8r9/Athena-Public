//! Ask other stations on the local network for files on a topic.
//!
//! A station that cannot answer a question from its own library sends the question to the
//! stations it has discovered. Each answers with titles and short passages, never whole files.
//! The user then picks what to copy, and the file arrives through the existing mesh download.
//!
//! Only shareable library files are ever offered: nothing from the private shelf, no group
//! memory, and nothing outside the library folders. When the group has a shared key, questions
//! must be signed with it, so a stranger on the network gets nothing.

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::path::Path;

use crate::db::Database;

const MAX_HITS: usize = 12;
/// A signed question is accepted for this long, so a recording of it cannot be replayed later.
const SIGNATURE_WINDOW_SECS: i64 = 300;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryHit {
    pub title: String,
    /// Path relative to the station's data folder, for the mesh download.
    pub rel_path: String,
    pub file_type: String,
    pub bytes: u64,
    /// A short passage that matched.
    pub snippet: String,
}

/// Words worth searching for: three letters or more, no filler, at most six.
pub fn query_terms(query: &str) -> Vec<String> {
    const FILLER: &[&str] = &[
        "the", "and", "for", "how", "what", "where", "when", "why", "who", "does", "anyone", "have", "has", "any", "about",
        "with", "from", "that", "this", "can", "you", "are", "there", "files", "file", "info", "information", "something",
    ];
    let mut out: Vec<String> = Vec::new();
    for w in query.to_lowercase().split(|c: char| !c.is_alphanumeric()) {
        if w.len() >= 3 && !FILLER.contains(&w) && !out.iter().any(|x| x == w) {
            out.push(w.to_string());
        }
    }
    out.truncate(6);
    out
}

/// True for an indexed path that may be offered to other stations.
pub fn is_shareable(path: &str) -> bool {
    let p = path.replace('\\', "/");
    !super::scan::is_private_path(&p)
        && !p.contains("/group-memory/")
        && !p.contains(".zim::")
        && !p.starts_with(super::packs::INDEX_SCHEME)
}

/// Paths under the data folder that are never served to another station, whoever asks.
pub fn is_served_path_forbidden(rel: &str) -> bool {
    let p = rel.replace('\\', "/").to_lowercase();
    let p = p.trim_start_matches('/');
    let private = super::scan::PRIVATE_SHELF.to_lowercase();
    p.starts_with("user/db/")
        || p.starts_with("user/config/")
        || p.starts_with("user/chats/")
        || p.starts_with("app/")
        || p.contains(&format!("/{private}/"))
        || p.ends_with(".db")
        || p.ends_with(".db-wal")
        || p.ends_with(".db-shm")
        || p.ends_with(".sqlite")
        || p.ends_with("config.json")
}

/// Search this station's shareable library for `query`: titles first, then passages.
pub fn search_local(db: &Database, data_root: &Path, query: &str) -> Vec<LibraryHit> {
    let terms = query_terms(query);
    if terms.is_empty() {
        return Vec::new();
    }
    let root = data_root.to_string_lossy().replace('\\', "/");
    let rows: Vec<(String, String, String, i64, String)> = db
        .with_conn(|conn| {
            let mut out = Vec::new();
            // Titles and file names.
            let mut stmt = conn
                .prepare(
                    "SELECT path, COALESCE(NULLIF(display_title, ''), name), file_type, COALESCE(size, 0)
                     FROM indexed_files WHERE instr(lower(COALESCE(display_title, '') || ' ' || name), ?1) > 0 LIMIT 40",
                )
                .map_err(|e| e.to_string())?;
            for term in &terms {
                let rows = stmt
                    .query_map([term], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?)))
                    .map_err(|e| e.to_string())?;
                for (path, title, ft, size) in rows.flatten() {
                    out.push((path, title, ft, size, String::new()));
                }
            }
            // Passages, through the full-text index over the chat chunks.
            let fts = terms.iter().map(|t| format!("\"{t}\"")).collect::<Vec<_>>().join(" OR ");
            let passages = conn.prepare(
                "SELECT ce.file_path, COALESCE(NULLIF(f.display_title, ''), f.name, ce.file_path), COALESCE(f.file_type, ''), COALESCE(f.size, 0),
                        snippet(library_chunks_fts, 0, '', '', ' … ', 24)
                 FROM library_chunks_fts JOIN chunk_embeddings ce ON ce.id = library_chunks_fts.rowid
                 LEFT JOIN indexed_files f ON f.path = ce.file_path
                 WHERE library_chunks_fts MATCH ?1 ORDER BY rank LIMIT 60",
            );
            if let Ok(mut stmt) = passages {
                if let Ok(rows) = stmt.query_map([fts], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?, r.get::<_, String>(4)?))
                }) {
                    out.extend(rows.flatten());
                }
            }
            Ok(out)
        })
        .unwrap_or_default();

    let mut hits: Vec<(u32, LibraryHit)> = Vec::new();
    for (path, title, file_type, size, snippet) in rows {
        let norm = path.replace('\\', "/");
        if !is_shareable(&norm) {
            continue;
        }
        let Some(rel) = norm.strip_prefix(&root).map(|r| r.trim_start_matches('/').to_string()) else { continue };
        if is_served_path_forbidden(&rel) {
            continue;
        }
        let lower_title = title.to_lowercase();
        let score = 3 * terms.iter().filter(|t| lower_title.contains(t.as_str())).count() as u32 + u32::from(!snippet.is_empty());
        if let Some(existing) = hits.iter_mut().find(|(_, h)| h.rel_path == rel) {
            existing.0 += u32::from(!snippet.is_empty());
            if existing.1.snippet.is_empty() {
                existing.1.snippet = snippet;
            }
            continue;
        }
        hits.push((score, LibraryHit { title, rel_path: rel, file_type, bytes: size.max(0) as u64, snippet: snippet.chars().take(300).collect() }));
    }
    hits.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.title.cmp(&b.1.title)));
    hits.into_iter().take(MAX_HITS).map(|(_, h)| h).collect()
}

/// Signature over a question, made with the group key.
pub fn sign_query(group_key: &[u8], query: &str, ts: i64) -> String {
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(group_key).expect("HMAC accepts any key length");
    mac.update(b"athena-mesh-library-v1|");
    mac.update(query.as_bytes());
    mac.update(b"|");
    mac.update(ts.to_string().as_bytes());
    mac.finalize().into_bytes().iter().map(|b| format!("{b:02x}")).collect()
}

/// Whether to answer a question. With a group key set, it must carry a fresh, valid signature.
pub fn authorize(group_key: Option<&[u8]>, query: &str, ts: i64, sig: &str, now: i64) -> Result<(), &'static str> {
    let Some(key) = group_key else { return Ok(()) };
    if (now - ts).abs() > SIGNATURE_WINDOW_SECS {
        return Err("The question is too old or this station's clock is wrong.");
    }
    let expected = sign_query(key, query, ts);
    let same = expected.len() == sig.len() && expected.bytes().zip(sig.bytes()).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0;
    if same {
        Ok(())
    } else {
        Err("The question is not signed with this group's key.")
    }
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: String,
    #[serde(default)]
    pub ts: i64,
    #[serde(default)]
    pub sig: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchAnswer {
    pub node_id: u32,
    pub station: String,
    pub hits: Vec<LibraryHit>,
}

/// `GET /mesh/library/search`: answer another station's question with titles and passages.
pub async fn http_search(
    axum::extract::State(s): axum::extract::State<crate::mesh_sync_api::MeshHttpState>,
    axum::extract::Query(q): axum::extract::Query<SearchQuery>,
) -> axum::response::Response {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    let now = chrono::Utc::now().timestamp();
    let result = tokio::task::spawn_blocking(move || -> Result<SearchAnswer, (StatusCode, String)> {
        let db = s.db.lock().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        if !super::load_settings(&db).mesh_share {
            return Err((StatusCode::FORBIDDEN, "This station is not sharing its library.".to_string()));
        }
        let trust = crate::field_ops::trust::load_settings(&db);
        let key = crate::field_ops::trust::key_bytes(&trust);
        authorize(key.as_deref(), &q.q, q.ts, &q.sig, now).map_err(|e| (StatusCode::FORBIDDEN, e.to_string()))?;
        Ok(SearchAnswer {
            node_id: s.node_id,
            station: trust.station_name.unwrap_or_else(|| format!("Station {}", s.node_id)),
            hits: search_local(&db, &s.athena_data_root, &q.q),
        })
    })
    .await;
    match result {
        Ok(Ok(answer)) => axum::Json(answer).into_response(),
        Ok(Err((code, msg))) => (code, msg).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn questions_reduce_to_their_meaningful_words() {
        assert_eq!(query_terms("Does anyone have files about alternator repair?"), vec!["alternator", "repair"]);
        assert!(query_terms("how do I").is_empty());
    }

    #[test]
    fn private_and_internal_files_are_never_offered() {
        assert!(is_shareable("/data/user/library/Medical/Where There Is No Doctor.pdf"));
        assert!(!is_shareable("/data/user/library/Private (this station only)/Local/cache.txt"));
        assert!(!is_shareable("/data/user/library/group-memory/north-well.md"));
        assert!(!is_shareable("athena-pack://athena-pack_x_2026-11-12.zim/a.pdf"));
        assert!(!is_shareable("/data/user/downloads/packages/wiki.zim::A/Water"));

        for bad in [
            "user/db/athena_index.db",
            "/user/db/athena_index.db-wal",
            "user/config/config.json",
            "user/chats/1.json",
            "user/library/Private (this station only)/Local/cache.txt",
        ] {
            assert!(is_served_path_forbidden(bad), "{bad}");
        }
        assert!(!is_served_path_forbidden("user/library/Medical/Where There Is No Doctor.pdf"));
        assert!(!is_served_path_forbidden("user/downloads/packages/wikipedia_en_medicine_nopic_2026-10.zim"));
    }

    #[test]
    fn a_group_key_makes_signed_fresh_questions_mandatory() {
        let key = [9u8; 32];
        let now = 1_794_000_000;
        let sig = sign_query(&key, "alternator repair", now);
        assert!(authorize(Some(&key), "alternator repair", now, &sig, now + 30).is_ok());
        assert!(authorize(Some(&key), "something else", now, &sig, now).is_err(), "signature covers the question");
        assert!(authorize(Some(&key), "alternator repair", now, &sig, now + 900).is_err(), "old questions are refused");
        assert!(authorize(Some(&[1u8; 32]), "alternator repair", now, &sig, now).is_err(), "another group's key fails");
        assert!(authorize(Some(&key), "alternator repair", now, "", now).is_err());
        assert!(authorize(None, "alternator repair", 0, "", now).is_ok(), "no group key: open on the local network");
    }
}
