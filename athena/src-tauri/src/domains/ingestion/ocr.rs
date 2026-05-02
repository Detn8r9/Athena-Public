//! Local OCR using Tesseract and pdftoppm as Tauri `externalBin` sidecars, with tessdata under
//! `resources/tessdata`. `init_ocr_environment` wires `TESSDATA_PREFIX` and validates `app.shell().sidecar(...)`.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::{io::Read, io::Result as IoResult};

use tauri::Manager;
use tauri_plugin_shell::ShellExt;

/// Max pages to OCR per PDF (pdftoppm may emit many PNGs; keep bounded for memory/time).
const SCANNED_PDF_MAX_PAGES: usize = 100;
const OCR_TEXT_MAX_LEN: usize = 2_000_000;

static OCR_TESSERACT: OnceLock<Option<PathBuf>> = OnceLock::new();
static OCR_PDFTOPPM: OnceLock<Option<PathBuf>> = OnceLock::new();
static TESSDATA_PREFIX: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Call from Tauri `setup`: resolve bundled tessdata, validate sidecars via `tauri-plugin-shell`, cache exe paths.
pub fn init_ocr_environment(app: &tauri::AppHandle) {
    let tessdata = app
        .path()
        .resolve("tessdata", tauri::path::BaseDirectory::Resource)
        .ok()
        .filter(|p: &PathBuf| p.is_dir());
    if let Some(ref p) = tessdata {
        eprintln!("[athena] OCR: tessdata resource dir: {}", p.display());
    } else {
        eprintln!(
            "[athena] OCR: tessdata missing under app resources (sync with scripts/sync_ocr_sidecars.sh)"
        );
    }
    let _ = TESSDATA_PREFIX.set(tessdata);

    match app.shell().sidecar("tesseract") {
        Ok(_) => eprintln!("[athena] OCR: tesseract sidecar OK (externalBin)"),
        Err(e) => eprintln!("[athena] OCR: tesseract sidecar error: {e}"),
    }
    match app.shell().sidecar("pdftoppm") {
        Ok(_) => eprintln!("[athena] OCR: pdftoppm sidecar OK (externalBin)"),
        Err(e) => eprintln!("[athena] OCR: pdftoppm sidecar error: {e}"),
    }

    let tess = bundled_sidecar_path("tesseract").or_else(|| which::which("tesseract").ok());
    let pdf = bundled_sidecar_path("pdftoppm").or_else(|| which::which("pdftoppm").ok());
    let _ = OCR_TESSERACT.set(tess);
    let _ = OCR_PDFTOPPM.set(pdf);
}

fn exe_dir_for_sidecars() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let mut dir = exe.parent()?.to_path_buf();
    if dir.ends_with("deps") {
        dir = dir.parent()?.to_path_buf();
    }
    Some(dir)
}

/// Same triple shape as `bundle.externalBin` suffix (`tesseract-{triple}` next to the app exe).
fn compiled_target_triple() -> &'static str {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "aarch64-apple-darwin"
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        "x86_64-apple-darwin"
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64", target_env = "musl")) {
        "x86_64-unknown-linux-musl"
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "x86_64-unknown-linux-gnu"
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        "aarch64-unknown-linux-gnu"
    } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        "x86_64-pc-windows-msvc"
    } else if cfg!(all(target_os = "windows", target_arch = "aarch64")) {
        "aarch64-pc-windows-msvc"
    } else {
        "unknown-unknown-unknown"
    }
}

fn bundled_sidecar_path(name: &str) -> Option<PathBuf> {
    let dir = exe_dir_for_sidecars()?;
    let triple = compiled_target_triple();
    let path = if cfg!(windows) {
        dir.join(format!("{}-{}.exe", name, triple))
    } else {
        dir.join(format!("{}-{}", name, triple))
    };
    if path.is_file() {
        Some(path)
    } else {
        None
    }
}

fn tesseract_binary() -> Option<PathBuf> {
    if let Some(locked) = OCR_TESSERACT.get() {
        return locked.clone();
    }
    bundled_sidecar_path("tesseract").or_else(|| which::which("tesseract").ok())
}

fn pdftoppm_binary() -> Option<PathBuf> {
    if let Some(locked) = OCR_PDFTOPPM.get() {
        return locked.clone();
    }
    bundled_sidecar_path("pdftoppm").or_else(|| which::which("pdftoppm").ok())
}

fn apply_tesseract_env(cmd: &mut Command) {
    if let Some(Some(ref p)) = TESSDATA_PREFIX.get() {
        cmd.env("TESSDATA_PREFIX", p);
    }
}

fn eprintln_ocr_stderr(tool: &str, stderr: &[u8]) {
    let stderr_str = String::from_utf8_lossy(stderr);
    if !stderr_str.trim().is_empty() {
        eprintln!("[athena] OCR Tool STDERR ({}): {}", tool, stderr_str);
    }
}

fn sorted_png_paths_from_pdftoppm(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = match std::fs::read_dir(dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.is_file()
                    && p.extension()
                        .and_then(|x| x.to_str())
                        .map(|s| s.eq_ignore_ascii_case("png"))
                        .unwrap_or(false)
            })
            .collect(),
        Err(_) => Vec::new(),
    };
    v.sort_by_key(|p| {
        p.file_stem()
            .and_then(|s| s.to_str())
            .and_then(|stem| stem.rsplit('-').next())
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0)
    });
    v.truncate(SCANNED_PDF_MAX_PAGES);
    v
}

fn read_to_string_capped(mut r: impl Read, cap: usize) -> IoResult<String> {
    let mut buf: Vec<u8> = Vec::with_capacity(cap.min(64 * 1024));
    let mut tmp = [0u8; 8192];
    while buf.len() < cap {
        let n = r.read(&mut tmp)?;
        if n == 0 {
            break;
        }
        let take = (cap - buf.len()).min(n);
        buf.extend_from_slice(&tmp[..take]);
        if take < n {
            break;
        }
    }
    Ok(String::from_utf8_lossy(&buf).to_string())
}

fn run_command_stdout_capped(mut cmd: Command, tool_label: &str, cap: usize) -> Option<String> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[athena] OCR: failed to spawn {tool_label}: {e}");
            return None;
        }
    };
    let mut stdout = child.stdout.take()?;
    let mut stderr = child.stderr.take()?;

    // Read stderr fully but cap to avoid log blowups.
    let mut stderr_buf: Vec<u8> = Vec::new();
    let mut stderr_tmp = [0u8; 8192];
    while stderr_buf.len() < 64 * 1024 {
        let n = stderr.read(&mut stderr_tmp).ok()?;
        if n == 0 { break; }
        let take = (64 * 1024 - stderr_buf.len()).min(n);
        stderr_buf.extend_from_slice(&stderr_tmp[..take]);
        if take < n { break; }
    }

    let stdout_str = read_to_string_capped(&mut stdout, cap).ok()?;
    let status = child.wait().ok()?;
    eprintln_ocr_stderr(tool_label, &stderr_buf);
    if !status.success() {
        eprintln!(
            "[athena] OCR: {tool_label} exited {:?}: {}",
            status.code(),
            String::from_utf8_lossy(&stderr_buf).trim()
        );
        return None;
    }
    let trimmed = stdout_str.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(if trimmed.len() > cap {
        trimmed[..cap].to_string()
    } else {
        trimmed.to_string()
    })
}

#[allow(dead_code)]
pub fn is_ocr_available() -> bool {
    tesseract_binary().is_some()
}

pub fn extract_text_from_image(path: &Path) -> Option<String> {
    let tesseract = tesseract_binary()?;
    if !path.is_file() {
        return None;
    }
    let path_str = path.to_string_lossy();
    let mut cmd = Command::new(&tesseract);
    apply_tesseract_env(&mut cmd);
    cmd.arg(path_str.as_ref()).arg("stdout");
    run_command_stdout_capped(cmd, "tesseract", OCR_TEXT_MAX_LEN)
}

pub fn extract_text_from_scanned_pdf(path: &Path) -> Option<String> {
    let pdftoppm = pdftoppm_binary()?;
    let tesseract = tesseract_binary()?;
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
    let mut pdf_cmd = Command::new(&pdftoppm);
    pdf_cmd
        .arg("-png")
        .arg("-r")
        .arg("150")
        .arg(path_str.as_ref())
        .arg(prefix_str.as_ref());
    // pdftoppm doesn't produce useful stdout for us; just cap capture and ignore return.
    let _ = run_command_stdout_capped(pdf_cmd, "pdftoppm", 8 * 1024);
    let image_count = std::fs::read_dir(&temp_dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.path().is_file())
                .count()
        })
        .unwrap_or(0);
    println!(
        "[athena] pdftoppm generated {} image files for OCR",
        image_count
    );
    let png_pages = sorted_png_paths_from_pdftoppm(&temp_dir);
    let mut combined = String::new();
    for (page_idx, page_path) in png_pages.iter().enumerate() {
        let page_num = page_idx + 1;
        println!(
            "[athena] Running Tesseract on page: {:?}",
            page_path.file_name().unwrap_or_default()
        );
        let page_str = page_path.to_string_lossy();
        let mut tess_cmd = Command::new(&tesseract);
        apply_tesseract_env(&mut tess_cmd);
        tess_cmd.arg(page_str.as_ref()).arg("stdout");
        let Some(page_text) = run_command_stdout_capped(tess_cmd, &format!("tesseract page {page_num}"), OCR_TEXT_MAX_LEN) else {
            continue;
        };
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
        eprintln!(
            "[athena] OCR: scanned PDF yielded no text (check pdftoppm output images and tesseract above)."
        );
        return None;
    }
    Some(combined)
}

struct TempDirGuard {
    path: PathBuf,
}

impl TempDirGuard {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
