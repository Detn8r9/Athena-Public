//! Development-only test bridge.
//!
//! Lets an ordinary browser tab drive the running desktop app, so the UI can be
//! screenshotted and exercised by automated tools that cannot see a native window.
//!
//! Safety:
//! - Compiled to a no-op in release builds (`debug_assertions` off).
//! - Starts only when `ATHENA_DEV_BRIDGE=1` is set in the environment.
//! - Listens on 127.0.0.1 only.
//!
//! How it works: the browser tab loads the Vite dev server with `?devbridge=1`. A small shim
//! (`src/app/devBridgeShim.ts`) stands in for Tauri's JS internals. Each `invoke` is POSTed
//! here, evaluated inside the real desktop webview with the real `invoke`, and the result comes
//! back through the `dev_bridge_reply` command. Backend events are forwarded over SSE.

use serde::Deserialize;
use serde_json::Value;

/// Called from the desktop webview with the outcome of a proxied `invoke`.
#[tauri::command]
pub fn dev_bridge_reply(id: u64, ok: bool, value: Value) {
    #[cfg(debug_assertions)]
    imp::deliver(id, ok, value);
    #[cfg(not(debug_assertions))]
    let _ = (id, ok, value);
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct InvokeBody {
    cmd: String,
    #[serde(default)]
    args: Value,
}

pub fn maybe_start(app: tauri::AppHandle) {
    #[cfg(debug_assertions)]
    {
        if std::env::var("ATHENA_DEV_BRIDGE").ok().as_deref() == Some("1") {
            imp::start(app);
        }
    }
    #[cfg(not(debug_assertions))]
    let _ = app;
}

#[cfg(debug_assertions)]
mod imp {
    use super::InvokeBody;
    use axum::extract::State;
    use axum::response::sse::{Event, KeepAlive, Sse};
    use axum::response::IntoResponse;
    use axum::routing::{get, post};
    use axum::{Json, Router};
    use futures_util::stream::{self, Stream};
    use serde_json::{json, Value};
    use std::collections::{HashMap, HashSet};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{mpsc as std_mpsc, Mutex, OnceLock};
    use std::time::Duration;
    use tauri::{AppHandle, Listener, Manager};
    use tokio::sync::mpsc;
    use tower_http::cors::{Any, CorsLayer};

    const PORT: u16 = 8799;

    static NEXT_ID: AtomicU64 = AtomicU64::new(1);

    fn pending() -> &'static Mutex<HashMap<u64, std_mpsc::Sender<(bool, Value)>>> {
        static P: OnceLock<Mutex<HashMap<u64, std_mpsc::Sender<(bool, Value)>>>> = OnceLock::new();
        P.get_or_init(|| Mutex::new(HashMap::new()))
    }

    fn sse_clients() -> &'static Mutex<Vec<mpsc::UnboundedSender<String>>> {
        static C: OnceLock<Mutex<Vec<mpsc::UnboundedSender<String>>>> = OnceLock::new();
        C.get_or_init(|| Mutex::new(Vec::new()))
    }

    fn subscribed() -> &'static Mutex<HashSet<String>> {
        static S: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
        S.get_or_init(|| Mutex::new(HashSet::new()))
    }

    pub fn deliver(id: u64, ok: bool, value: Value) {
        if let Some(tx) = pending().lock().ok().and_then(|mut p| p.remove(&id)) {
            let _ = tx.send((ok, value));
        }
    }

    fn publish(event: &str, payload: Value) {
        let data = json!({ "event": event, "payload": payload }).to_string();
        if let Ok(mut clients) = sse_clients().lock() {
            clients.retain(|tx| tx.send(data.clone()).is_ok());
        }
    }

    fn proxy_invoke(app: &AppHandle, cmd: &str, args: &Value) -> Result<Value, String> {
        let webview = app
            .get_webview_window("main")
            .ok_or_else(|| "dev bridge: main window not found".to_string())?;
        let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = std_mpsc::channel();
        pending().lock().map_err(|e| e.to_string())?.insert(id, tx);
        let cmd_js = serde_json::to_string(cmd).map_err(|e| e.to_string())?;
        let args_js = serde_json::to_string(args).map_err(|e| e.to_string())?;
        let script = format!(
            r#"(function(){{
  const I = window.__TAURI_INTERNALS__;
  const reply = (ok, value) => I.invoke('dev_bridge_reply', {{ id: {id}, ok, value: value === undefined ? null : value }})
    .catch(() => I.invoke('dev_bridge_reply', {{ id: {id}, ok: false, value: 'dev bridge: result not serialisable' }}));
  I.invoke({cmd_js}, {args_js}).then(v => reply(true, v), e => reply(false, (e && e.message) ? e.message : String(e)));
}})();"#
        );
        webview.eval(&script).map_err(|e| e.to_string())?;
        match rx.recv_timeout(Duration::from_secs(600)) {
            Ok((true, v)) => Ok(v),
            Ok((false, v)) => Err(match v {
                Value::String(s) => s,
                other => other.to_string(),
            }),
            Err(_) => {
                if let Ok(mut p) = pending().lock() {
                    p.remove(&id);
                }
                Err(format!("dev bridge: `{cmd}` timed out"))
            }
        }
    }

    async fn invoke_handler(State(app): State<AppHandle>, Json(body): Json<InvokeBody>) -> impl IntoResponse {
        let cmd = body.cmd.clone();
        let args = body.args.clone();
        let res = tokio::task::spawn_blocking(move || proxy_invoke(&app, &cmd, &args))
            .await
            .unwrap_or_else(|e| Err(e.to_string()));
        match res {
            Ok(v) => Json(json!({ "ok": true, "result": v })),
            Err(e) => Json(json!({ "ok": false, "error": e })),
        }
    }

    #[derive(serde::Deserialize)]
    struct SubscribeBody {
        event: String,
    }

    async fn subscribe_handler(State(app): State<AppHandle>, Json(body): Json<SubscribeBody>) -> impl IntoResponse {
        let name = body.event;
        let fresh = subscribed().lock().map(|mut s| s.insert(name.clone())).unwrap_or(false);
        if fresh {
            let ev_name = name.clone();
            app.listen_any(name, move |e| {
                let raw = e.payload().to_string();
                let payload = serde_json::from_str::<Value>(&raw).unwrap_or(Value::String(raw));
                publish(&ev_name, payload);
            });
        }
        Json(json!({ "ok": true }))
    }

    async fn events_handler() -> Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>> {
        let (tx, rx) = mpsc::unbounded_channel::<String>();
        if let Ok(mut c) = sse_clients().lock() {
            c.push(tx);
        }
        let s = stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|d| (Ok(Event::default().data(d)), rx))
        });
        Sse::new(s).keep_alive(KeepAlive::default())
    }

    pub fn start(app: AppHandle) {
        tauri::async_runtime::spawn(async move {
            let cors = CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any);
            let router = Router::new()
                .route("/invoke", post(invoke_handler))
                .route("/subscribe", post(subscribe_handler))
                .route("/events", get(events_handler))
                .with_state(app)
                .layer(cors);
            let addr = std::net::SocketAddr::from(([127, 0, 0, 1], PORT));
            match tokio::net::TcpListener::bind(addr).await {
                Ok(listener) => {
                    eprintln!("[athena-dev-bridge] listening on http://{addr} (debug build, ATHENA_DEV_BRIDGE=1)");
                    if let Err(e) = axum::serve(listener, router.into_make_service()).await {
                        eprintln!("[athena-dev-bridge] stopped: {e}");
                    }
                }
                Err(e) => eprintln!("[athena-dev-bridge] bind {addr} failed: {e}"),
            }
        });
    }
}
