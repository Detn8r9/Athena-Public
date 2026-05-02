//! LAN mesh P2P REST: Axum on `0.0.0.0` with graceful shutdown, `/mesh/ping`, `/mesh/manifest`,
//! and streaming `/mesh/download` (strictly under AthenaData root).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use axum::body::Body;
use axum::extract::State;
use axum::http::header;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Json;
use axum::Router;
use chrono::TimeZone;
use serde::Serialize;
use tokio::net::TcpListener;
use tokio_util::io::ReaderStream;
use tokio_util::sync::CancellationToken;

use crate::db::{Database, MeshManifestRow};

/// True when `url` is `http` and the host is localhost, `.local`, loopback, link-local, or RFC1918 IPv4.
pub fn mesh_manifest_url_is_lan_safe(url: &reqwest::Url) -> bool {
    if url.scheme() != "http" {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    if host.ends_with(".local") {
        return true;
    }
    if let Ok(ip) = host.parse::<std::net::Ipv4Addr>() {
        return ip.is_private() || ip.is_loopback() || ip.is_link_local();
    }
    false
}

static MESH_P2P_CANCEL: OnceLock<CancellationToken> = OnceLock::new();

pub fn mesh_p2p_shutdown_token() -> CancellationToken {
    MESH_P2P_CANCEL
        .get_or_init(|| CancellationToken::new())
        .clone()
}

/// Signal the mesh Axum server to shut down (call from [`RunEvent::Exit`]).
pub fn signal_mesh_p2p_shutdown() {
    if let Some(t) = MESH_P2P_CANCEL.get() {
        t.cancel();
    }
}

#[derive(Clone)]
pub struct MeshHttpState {
    pub db: Arc<Mutex<Database>>,
    pub node_id: u32,
    pub athena_data_root: PathBuf,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeshPingResponse {
    pub ok: bool,
    pub node_id: u32,
    pub service: &'static str,
}

async fn mesh_ping(State(s): State<MeshHttpState>) -> Json<MeshPingResponse> {
    Json(MeshPingResponse {
        ok: true,
        node_id: s.node_id,
        service: "athena-mesh-p2p",
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeshManifestBody {
    pub node_id: u32,
    pub generated_at_rfc3339: String,
    pub entry_count: usize,
    pub entries: Vec<MeshManifestRow>,
}

async fn mesh_manifest(State(s): State<MeshHttpState>) -> impl IntoResponse {
    let db = s.db.clone();
    let node_id = s.node_id;
    let json_bytes: Result<Vec<u8>, String> = tokio::task::spawn_blocking(move || {
        let g = db.lock().map_err(|e| e.to_string())?;
        let entries = g.list_mesh_manifest_rows(50_000)?;
        let body = MeshManifestBody {
            node_id,
            generated_at_rfc3339: chrono::Utc::now().to_rfc3339(),
            entry_count: entries.len(),
            entries,
        };
        serde_json::to_vec(&body).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|x| x);

    match json_bytes {
        Ok(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(bytes))
            .unwrap()
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            e,
        )
            .into_response(),
    }
}

#[derive(serde::Deserialize)]
pub struct MeshDownloadQuery {
    /// URL-encoded relative path under AthenaData (POSIX slashes).
    pub p: String,
}

/// Join `rel` under `root` with strict traversal checks (no `..`, no prefixes outside root).
pub fn safe_path_under_athena_root(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let decoded = urlencoding::decode(rel).map_err(|_| "Invalid URL-encoded path.".to_string())?;
    let s = decoded.trim();
    if s.is_empty() {
        return Err("Empty path.".to_string());
    }
    let norm = s.replace('\\', "/");
    if norm.contains("..") {
        return Err("Path traversal is not allowed.".to_string());
    }
    let root_canon = std::fs::canonicalize(root).map_err(|e| format!("AthenaData root: {e}"))?;
    let mut acc = root_canon.clone();
    for seg in norm.split('/').filter(|x| !x.is_empty() && *x != ".") {
        acc.push(seg);
    }
    if !acc.starts_with(&root_canon) {
        return Err("Resolved path escapes AthenaData root.".to_string());
    }
    Ok(acc)
}

fn http_last_modified_gmt(st: std::time::SystemTime) -> Option<axum::http::HeaderValue> {
    let secs = st.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64;
    let dt = chrono::Utc.timestamp_opt(secs, 0).single()?;
    let s = dt.format("%a, %d %b %Y %H:%M:%S GMT").to_string();
    axum::http::HeaderValue::from_str(&s).ok()
}

async fn mesh_download(
    State(s): State<MeshHttpState>,
    axum::extract::Query(q): axum::extract::Query<MeshDownloadQuery>,
) -> impl IntoResponse {
    let root = s.athena_data_root.clone();
    let rel = q.p.clone();
    let resolved = match tokio::task::spawn_blocking(move || safe_path_under_athena_root(&root, &rel)).await {
        Ok(Ok(p)) => p,
        Ok(Err(e)) => {
            return (StatusCode::BAD_REQUEST, e).into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("join task failed: {e}"),
            )
                .into_response();
        }
    };

    let meta = match tokio::fs::metadata(&resolved).await {
        Ok(m) => m,
        Err(_) => return (StatusCode::NOT_FOUND, "not found").into_response(),
    };
    if !meta.is_file() {
        return (StatusCode::BAD_REQUEST, "not a file").into_response();
    }

    let file = match tokio::fs::File::open(&resolved).await {
        Ok(f) => f,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("open failed: {e}"),
            )
                .into_response();
        }
    };

    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    let len = meta.len();
    let mut res = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_LENGTH, len.to_string());
    if let Ok(mtime) = meta.modified() {
        if let Some(hv) = http_last_modified_gmt(mtime) {
            res = res.header(header::LAST_MODIFIED, hv);
        }
    }
    res.body(body).unwrap().into_response()
}

pub fn spawn_mesh_http_server(
    db: Arc<Mutex<Database>>,
    athena_data_root: PathBuf,
    port_tx: tokio::sync::oneshot::Sender<u16>,
) {
    let cancel = mesh_p2p_shutdown_token();
    tauri::async_runtime::spawn(async move {
        let listener = match TcpListener::bind("0.0.0.0:0").await {
            Ok(l) => l,
            Err(e) => {
                eprintln!("[mesh-p2p] TCP bind failed: {e}");
                return;
            }
        };
        let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
        if port_tx.send(port).is_err() {
            eprintln!("[mesh-p2p] port channel closed");
            return;
        }

        let node_id = crate::lan_identity::lan_local_node_id();
        let state = MeshHttpState {
            db,
            node_id,
            athena_data_root,
        };
        let router = Router::new()
            .route("/mesh/ping", get(mesh_ping))
            .route("/mesh/manifest", get(mesh_manifest))
            .route("/mesh/download", get(mesh_download))
            .with_state(state);

        eprintln!("[mesh-p2p] REST listening on 0.0.0.0:{port}");
        let shutdown = {
            let c = cancel.clone();
            async move {
                c.cancelled().await;
            }
        };
        if let Err(e) = axum::serve(listener, router.into_make_service())
            .with_graceful_shutdown(shutdown)
            .await
        {
            eprintln!("[mesh-p2p] server stopped: {e}");
        }
    });
}

pub fn spawn_mesh_p2p_stack(
    app: tauri::AppHandle,
    db: Arc<Mutex<Database>>,
    athena_data_root: PathBuf,
) {
    let (tx, rx) = tokio::sync::oneshot::channel::<u16>();
    spawn_mesh_http_server(db, athena_data_root, tx);
    tauri::async_runtime::spawn(async move {
        if let Ok(p) = rx.await {
            if p > 0 {
                crate::mesh_discovery::spawn_mesh_discovery(app, p);
            }
        }
    });
}
