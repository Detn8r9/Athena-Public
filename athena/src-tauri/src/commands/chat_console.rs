use std::sync::Arc;

use tauri::{Emitter, Manager};

use crate::config::load_config;
use crate::console_chat_library_context;
use crate::inference::{chat_stream_unified, ensure_inference_ready, load_models_for_inference};
use crate::ollama::ChatMessage;
use crate::AppState;
use crate::ATHENA_FULL_NAME;

// Console chat stream: every `chat_console_stream` call embeds `search_similar_chunks` RAG hits into
// `Local_library_context` before `chat_stream_unified` (same pattern as other RAG-wrapped Athena chat paths).

const CONSOLE_STREAM_PHASE_EVENT: &str = "athena:console-stream-phase";
const CONSOLE_STREAM_CHUNK_EVENT: &str = "athena:console-stream-chunk";
const CONSOLE_STREAM_DONE_EVENT: &str = "athena:console-stream-done";
const CONSOLE_STREAM_ERROR_EVENT: &str = "athena:console-stream-error";

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ConsoleStreamPhasePayload {
    phase: String,
}

#[tauri::command]
pub fn chat_console_stream(app: tauri::AppHandle, message: String) -> Result<(), String> {
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
    let app_emit = app.clone();
    let q_string = q.to_string();

    std::thread::spawn(move || {
        let config = load_config(&app_data_dir);
        let models = load_models_for_inference(&config);

        let _ = app_emit.emit(
            CONSOLE_STREAM_PHASE_EVENT,
            ConsoleStreamPhasePayload {
                phase: "searching".to_string(),
            },
        );

        let (rag_context, stream_sources) = {
            let db = match db_arc.lock() {
                Ok(g) => g,
                Err(e) => {
                    let _ = app_emit.emit(
                        CONSOLE_STREAM_ERROR_EVENT,
                        format!("Database lock failed: {e}"),
                    );
                    let _ = app_emit.emit(CONSOLE_STREAM_DONE_EVENT, ());
                    return;
                }
            };
            let rag = console_chat_library_context(&*db, &q_string);
            let cites: Vec<crate::StreamLibrarySource> = match db.hybrid_search(&q_string) {
                Ok(hits) if !hits.is_empty() => hits
                    .iter()
                    .map(crate::hybrid_hit_to_stream_source)
                    .collect(),
                _ => Vec::new(),
            };
            (rag, cites)
        };

        if !stream_sources.is_empty() {
            let _ = app_emit.emit(crate::STREAM_SOURCES_EVENT, &stream_sources);
        }

        let system_prompt = format!(
            "You are Athena, the assistant for {}. You are an advanced offline knowledge base assistant. \
Be highly analytical and direct. \
Use the provided Local_library_context as evidence. \
If the context is insufficient or irrelevant, say so explicitly and answer using general knowledge without fabricating details. \
When giving procedural advice, provide a short checklist and call out risks and unknowns.",
            ATHENA_FULL_NAME
        );

        let user_prompt = format!(
            "Local_library_context:\n{}\n\nUser_query:\n{}",
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

        let mut on_chunk = |chunk: &str| {
            let _ = app_emit.emit(CONSOLE_STREAM_CHUNK_EVENT, chunk);
        };
        let on_done = || {
            let _ = app_emit.emit(CONSOLE_STREAM_DONE_EVENT, ());
        };

        if let Err(e) =
            chat_stream_unified(&config, &models, messages, &mut on_chunk, on_done, None)
        {
            let _ = app_emit.emit(CONSOLE_STREAM_ERROR_EVENT, e);
            let _ = app_emit.emit(CONSOLE_STREAM_DONE_EVENT, ());
        }
    });

    Ok(())
}
