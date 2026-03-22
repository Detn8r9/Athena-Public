//! Local OCR fallback using Tesseract (and pdftoppm for scanned PDFs).
//! Runs entirely offline when the tools are installed on the system.

use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

const TESSERACT_BIN: &str = "tesseract";
const PDFTOPPM_BIN: &str = "pdftoppm";
const SCANNED_PDF_MAX_PAGES: usize = 50;
const OCR_TEXT_MAX_LEN: usize = 2_000_000;

static TESSERACT_PATH: OnceLock<Option<std::path::PathBuf>> = OnceLock::new();
static PDFTOPPM_PATH: OnceLock<Option<std::path::PathBuf>> = OnceLock::new();

fn find_tesseract_cached() -> Option<&'static std::path::Path> {
    TESSERACT_PATH
        .get_or_init(|| which::which(TESSERACT_BIN).ok())
        .as_deref()
}

fn find_pdftoppm_cached() -> Option<&'static std::path::Path> {
    PDFTOPPM_PATH
        .get_or_init(|| which::which(PDFTOPPM_BIN).ok())
        .as_deref()
}

/// Returns true if Tesseract is available on the system for local OCR.
#[allow(dead_code)] // Reserved for settings / diagnostics when OCR is wired in the UI
pub fn is_ocr_available() -> bool {
    find_tesseract_cached().is_some()
}

/// Extracts text from an image file using Tesseract. Returns None if Tesseract
/// is not installed, the process fails, or the output is empty.
pub fn extract_text_from_image(path: &Path) -> Option<String> {
    let tesseract = find_tesseract_cached()?;
    if !path.is_file() {
        return None;
    }
    let path_str = path.to_string_lossy();
    let output = Command::new(tesseract)
        .arg(path_str.as_ref())
        .arg("stdout")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let result = if trimmed.len() > OCR_TEXT_MAX_LEN {
        trimmed[..OCR_TEXT_MAX_LEN].to_string()
    } else {
        trimmed.to_string()
    };
    Some(result)
}

/// Extracts text from a PDF by rendering pages to images and running OCR.
/// Used when the PDF has no or negligible embedded text (e.g. scanned documents).
/// Requires both pdftoppm (poppler-utils) and tesseract to be installed.
pub fn extract_text_from_scanned_pdf(path: &Path) -> Option<String> {
    let pdftoppm = find_pdftoppm_cached()?;
    let tesseract = find_tesseract_cached()?;
    if !path.is_file() {
        return None;
    }
    let temp_dir = std::env::temp_dir().join(format!("athena_ocr_{}", std::process::id()));
    if std::fs::create_dir_all(&temp_dir).is_err() {
        return None;
    }
    let _cleanup = TempDirGuard::new(temp_dir.clone());
    let path_str = path.to_string_lossy();
    let prefix = temp_dir.join("page");
    let prefix_str = prefix.to_string_lossy();
    let out = Command::new(pdftoppm)
        .arg("-png")
        .arg("-r")
        .arg("150")
        .arg(path_str.as_ref())
        .arg(prefix_str.as_ref())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let mut combined = String::new();
    for page_num in 1..=SCANNED_PDF_MAX_PAGES {
        let page_path = temp_dir.join(format!("page-{}.png", page_num));
        if !page_path.exists() {
            break;
        }
        let page_str = page_path.to_string_lossy();
        let cmd_out = match Command::new(tesseract)
            .arg(page_str.as_ref())
            .arg("stdout")
            .output()
        {
            Ok(o) => o,
            Err(_) => continue,
        };
        if !cmd_out.status.success() {
            continue;
        }
        let page_text = String::from_utf8_lossy(&cmd_out.stdout).trim().to_string();
        if !page_text.is_empty() {
            if !combined.is_empty() {
                combined.push_str("\n\n");
            }
            combined.push_str(&page_text);
        }
        if combined.len() > OCR_TEXT_MAX_LEN {
            combined.truncate(OCR_TEXT_MAX_LEN);
            break;
        }
    }
    if combined.trim().is_empty() {
        return None;
    }
    Some(combined)
}

struct TempDirGuard {
    path: std::path::PathBuf,
}

impl TempDirGuard {
    fn new(path: std::path::PathBuf) -> Self {
        Self { path }
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
