//! Console chat: library retrieval via [`crate::Database::hybrid_search`], then streaming inference
//! with Tauri events. Orchestration uses `tokio` + `spawn_blocking` so the async runtime is not used
//! for blocking SQLite, embeddings, or HTTP stream reads.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;

use tauri::Emitter;
use tauri::Manager;

use crate::config::load_config;
use crate::inference::{chat_stream_unified, ensure_inference_ready, load_models_for_inference};
use crate::ollama::ChatMessage;
use crate::AppState;
use crate::ATHENA_FULL_NAME;

pub const CONSOLE_STREAM_PHASE_EVENT: &str = "athena:console-stream-phase";
pub const CONSOLE_STREAM_CHUNK_EVENT: &str = "athena:console-stream-chunk";
pub const CONSOLE_STREAM_DONE_EVENT: &str = "athena:console-stream-done";
pub const CONSOLE_STREAM_ERROR_EVENT: &str = "athena:console-stream-error";

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsoleStreamPhasePayload {
    pub phase: String,
}

/// Fire-and-forget console stream: validates input, then schedules async work.
pub fn start_chat_console_stream(app: tauri::AppHandle, message: String) -> Result<(), String> {
    let q = message.trim();
    if q.is_empty() {
        return Err("Type a message first.".to_string());
    }

    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let config = load_config(&app_data_dir);
    if config.ai_inference_sleep.unwrap_or(false) {
        return Err(
            "Local AI is asleep. Meshtastic /ai and mesh auto-replies still run when enabled. Wake the AI from the lock screen or header."
                .to_string(),
        );
    }
    let models = load_models_for_inference(&config);
    ensure_inference_ready(&config, &models)?;

    let state = app.state::<AppState>();
    let db_arc = Arc::clone(&state.db);
    let cancel_arc = Arc::clone(&state.console_stream_cancel);
    cancel_arc.store(false, Ordering::SeqCst);

    let app_emit = app.clone();
    let q_string = q.to_string();
    let models_for_stream = models.clone();

    tauri::async_runtime::spawn(async move {
        let app_data_dir = match crate::portable::resolved_app_data_dir(&app_emit) {
            Ok(p) => p,
            Err(e) => {
                let _ = app_emit.emit(CONSOLE_STREAM_ERROR_EVENT, e);
                let _ = app_emit.emit(CONSOLE_STREAM_DONE_EVENT, ());
                return;
            }
        };
        let config = load_config(&app_data_dir);
        let models = load_models_for_inference(&config);
        if let Err(e) = ensure_inference_ready(&config, &models) {
            let _ = app_emit.emit(CONSOLE_STREAM_ERROR_EVENT, e);
            let _ = app_emit.emit(CONSOLE_STREAM_DONE_EVENT, ());
            return;
        }

        let skip_rag = crate::library_rag_gate::should_skip_library_rag(&q_string, false);
        if !skip_rag {
            let _ = app_emit.emit(
                CONSOLE_STREAM_PHASE_EVENT,
                ConsoleStreamPhasePayload {
                    phase: "searching".to_string(),
                },
            );
        }

        let db_bg = Arc::clone(&db_arc);
        let q_bg = q_string.clone();
        let rag_prepare = tokio::task::spawn_blocking(move || {
            if crate::library_rag_gate::should_skip_library_rag(&q_bg, false) {
                return Ok::<_, String>((String::new(), Vec::<crate::StreamLibrarySource>::new()));
            }
            let db = db_bg.lock().map_err(|e| e.to_string())?;
            // One retrieval pass: reranked, relevance-filtered, sized to the default context window.
            let hits = db.hybrid_search(&q_bg)?;
            let budget = crate::library_context::library_context_char_budget(None, q_bg.len());
            let assembled = crate::library_context::assemble_library_context(&hits, budget);
            let rag_block = assembled.body;
            let cites = assembled.sources;
            Ok::<_, String>((rag_block, cites))
        })
        .await;

        let (rag_context, stream_sources) = match rag_prepare {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => {
                let _ = app_emit.emit(CONSOLE_STREAM_ERROR_EVENT, e);
                let _ = app_emit.emit(CONSOLE_STREAM_DONE_EVENT, ());
                return;
            }
            Err(e) => {
                let _ = app_emit.emit(CONSOLE_STREAM_ERROR_EVENT, e.to_string());
                let _ = app_emit.emit(CONSOLE_STREAM_DONE_EVENT, ());
                return;
            }
        };

        if !stream_sources.is_empty() {
            let _ = app_emit.emit(crate::STREAM_SOURCES_EVENT, &stream_sources);
        }

        let system_prompt = format!(
            "You are ATHENA, a local AI assistant for {}. You have access to a local knowledge base. \
1. If the provided context is directly relevant to the user's question, use it to answer. \
2. If the context is irrelevant or absent, answer using your general knowledge, but mention that you are not using a local source for that specific info. \
If you are not using local sources, do NOT thank, cite, or credit the provided context.",
            ATHENA_FULL_NAME
        );

        let user_prompt = format!(
            "Library excerpts (may be empty):\n{}\n\nUser question:\n{}",
            rag_context, q_string
        );

        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: system_prompt,
            images: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: user_prompt,
            images: None,
            },
        ];

        let _ = app_emit.emit(
            CONSOLE_STREAM_PHASE_EVENT,
            ConsoleStreamPhasePayload {
                phase: "generating".to_string(),
            },
        );

        let cancel_for_chunks = Arc::clone(&cancel_arc);
        let cancel_for_inference = Arc::clone(&cancel_arc);
        let app_stream = app_emit.clone();
        let config_stream = config.clone();
        let models_stream = models_for_stream;

        let gen_result = tokio::task::spawn_blocking(move || {
            let mut chunk_count: u32 = 0;
            let mut on_chunk = |chunk: &str| {
                if cancel_for_chunks.load(Ordering::SeqCst) {
                    return;
                }
                chunk_count = chunk_count.wrapping_add(1);
                if chunk_count % 8 == 0 {
                    thread::yield_now();
                }
                let _ = app_stream.emit(CONSOLE_STREAM_CHUNK_EVENT, chunk);
            };
            let on_done = || {
                let _ = app_stream.emit(CONSOLE_STREAM_DONE_EVENT, ());
            };
            chat_stream_unified(
                &config_stream,
                &models_stream,
                messages,
                &mut on_chunk,
                on_done,
                None,
                None,
                Some(cancel_for_inference),
            )
        })
        .await;

        match gen_result {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                let _ = app_emit.emit(CONSOLE_STREAM_ERROR_EVENT, e);
                let _ = app_emit.emit(CONSOLE_STREAM_DONE_EVENT, ());
            }
            Err(e) => {
                let _ = app_emit.emit(CONSOLE_STREAM_ERROR_EVENT, e.to_string());
                let _ = app_emit.emit(CONSOLE_STREAM_DONE_EVENT, ());
            }
        }
    });

    Ok(())
}
