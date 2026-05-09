//! Console chat: RAG via [`crate::rag_indexer::search_rag_chunks_blocking`], then streaming inference
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

fn format_rag_context(results: &[crate::rag_indexer::RagSearchResult]) -> String {
    if results.is_empty() {
        return "No semantic RAG chunks matched the query in the vector index.".to_string();
    }
    let mut out =
        String::from("Retrieved chunks (embedding + sqlite-vec retrieval; use as primary evidence):\n");
    for (i, r) in results.iter().enumerate() {
        let body: String = r.content.chars().take(8000).collect();
        out.push_str(&format!(
            "--- Source {} score={:.4} doc={} chunk={} ---\n{}\n\n",
            i + 1,
            r.score,
            r.document_id,
            r.chunk_index,
            body
        ));
    }
    out
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

        let _ = app_emit.emit(
            CONSOLE_STREAM_PHASE_EVENT,
            ConsoleStreamPhasePayload {
                phase: "searching".to_string(),
            },
        );

        let db_bg = Arc::clone(&db_arc);
        let q_bg = q_string.clone();
        let rag_prepare = tokio::task::spawn_blocking(move || {
            let db = db_bg.lock().map_err(|e| e.to_string())?;
            let rag_hits = crate::rag_indexer::search_rag_chunks_blocking(&*db, &q_bg, 12)?;
            let rag_text = format_rag_context(&rag_hits);
            let hybrid_block = crate::console_chat_library_context(&*db, &q_bg);
            let rag_block = format!(
                "{}\n\nKeyword_and_hybrid_library_context:\n{}",
                rag_text, hybrid_block
            );
            // Only emit sources when the semantic RAG thresholding produced at least one usable chunk.
            let cites: Vec<crate::StreamLibrarySource> = if rag_hits.is_empty() {
                Vec::new()
            } else {
                match db.hybrid_search(&q_bg) {
                    Ok(hits) if !hits.is_empty() => hits
                        .iter()
                        .map(crate::hybrid_hit_to_stream_source)
                        .collect(),
                    _ => Vec::new(),
                }
            };
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
            "Semantic_RAG_context:\n{}\n\nUser_query:\n{}",
            rag_context, q_string
        );

        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: system_prompt,
            },
            ChatMessage {
                role: "user".to_string(),
                content: user_prompt,
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
