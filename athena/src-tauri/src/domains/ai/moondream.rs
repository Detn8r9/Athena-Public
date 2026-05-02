//! On-device vision (Moondream-style) — adapter hook. Full ONNX/Candle integration is a larger follow-up.
//! Callers must keep images on-device; no network upload.

/// Placeholder until a runtime (ONNX, sidecar) is wired. Returns a clear message for the UI.
pub fn describe_image_stub(_image_path: &std::path::Path) -> Result<String, String> {
    Err(
        "On-device vision is not enabled in this build yet. Images stay local; add a Moondream/ONNX runtime in a future update."
            .to_string(),
    )
}
