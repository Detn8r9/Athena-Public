#![allow(dead_code)]

//! Future universal SDR support (RTL-SDR, HackRF, etc.).
//!
//! Planned integration: a host library such as [SoapySDR](https://github.com/pothosware/SoapySDR)
//! to abstract USB dongles and expose a single demod/listen pipeline for the "Broadcast Listen"
//! Communications Hub card. This module is a stub until that stack is wired in.
//!
//! Note: [`crate::sdr_manager`] already handles RTL-SDR via `rtl_fm` for raw PCM; this file is the
//! long-term home for in-process / multi-device probing.

/// Placeholder return type for a future probe implementation.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SdrStubInfo {
    pub ready: bool,
}

/// Stub: always reports no universal SDR hardware (SoapySDR path not compiled in yet).
pub fn probe_sdr_hardware() -> Result<SdrStubInfo, String> {
    Err("No SDR hardware detected (stub).".to_string())
}
