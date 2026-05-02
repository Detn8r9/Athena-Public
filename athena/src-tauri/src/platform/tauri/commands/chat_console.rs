use std::sync::atomic::Ordering;

use crate::AppState;

#[tauri::command]
pub fn chat_console_stream(app: tauri::AppHandle, message: String) -> Result<(), String> {
    crate::ai_console_chat::start_chat_console_stream(app, message)
}

#[tauri::command]
pub fn abort_chat_console_stream(state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.console_stream_cancel.store(true, Ordering::SeqCst);
    Ok(())
}
