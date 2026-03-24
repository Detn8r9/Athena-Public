//! macOS-only window tweaks to avoid an AppKit crash when using the green traffic-light button.
//!
//! On some macOS versions (including betas), native Spaces fullscreen triggers a transition that
//! can crash inside `CFStringCreateWithCString` with a NULL pointer (`_platform_strlen` at 0x0).
//!
//! AppKit expects **exactly one** of `FullScreenPrimary`, `FullScreenAuxiliary`, or `FullScreenNone`
//! in the window's collection behavior. We only removed `FullScreenPrimary` before, which can
//! leave the window in an ambiguous state where native fullscreen still starts. We now **set
//! [`NSWindowCollectionBehavior::FullScreenNone`]** explicitly and strip the fullscreen-capable
//! flags so the green button zooms/maximizes instead of entering Spaces fullscreen.
//!
//! In-app "fullscreen" uses [`tauri::WebviewWindow::set_simple_fullscreen`] so we still get a
//! borderless, menu-bar-hiding mode without the broken native fullscreen snapshot transition.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use objc2_app_kit::{NSWindow, NSWindowCollectionBehavior};
use tauri::WebviewWindow;

/// Tracks `set_simple_fullscreen` because Tauri's `is_fullscreen` only reflects native fullscreen.
pub struct MacosSimpleFullscreenState(pub Arc<AtomicBool>);

/// Disable native Spaces fullscreen: green button zooms; no `_NSEnterFullScreenTransitionController`.
pub fn apply_native_fullscreen_mitigation(window: &WebviewWindow) -> Result<(), String> {
    let ptr = window.ns_window().map_err(|e| e.to_string())?;
    if ptr.is_null() {
        return Err("NSWindow pointer was null".into());
    }
    // SAFETY: `ns_window()` is a valid `NSWindow` for the webview's lifetime. Call only on the
    // main thread (Tauri `setup` runs on the main thread; delayed re-applies use `run_on_main_thread`).
    unsafe {
        let ns_window: &NSWindow = &*ptr.cast();
        let mut behavior = ns_window.collectionBehavior();
        behavior.remove(
            NSWindowCollectionBehavior::FullScreenPrimary
                | NSWindowCollectionBehavior::FullScreenAuxiliary,
        );
        behavior.insert(NSWindowCollectionBehavior::FullScreenNone);
        ns_window.setCollectionBehavior(behavior);
    }
    Ok(())
}
