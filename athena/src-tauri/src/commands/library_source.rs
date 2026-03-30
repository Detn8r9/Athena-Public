use std::path::Path;
use std::process::Command;

#[cfg(target_os = "macos")]
fn file_url_with_page(abs: &Path, page: u32) -> Result<String, String> {
    let s = abs
        .to_str()
        .ok_or_else(|| "Path must be valid UTF-8 for PDF deep link.".to_string())?;
    let encoded: String = s
        .split('/')
        .map(|seg| {
            if seg.is_empty() {
                String::new()
            } else {
                urlencoding::encode(seg).into_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("/");
    let body = if s.starts_with('/') {
        format!("/{}", encoded.trim_start_matches('/'))
    } else {
        encoded
    };
    Ok(format!("file://{body}#page={page}"))
}

#[cfg(not(target_os = "macos"))]
fn open_non_macos(target: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        let p = target.to_str().ok_or_else(|| "non-utf8 path".to_string())?;
        let st = Command::new("cmd")
            .args(["/C", "start", "", p])
            .status()
            .map_err(|e| e.to_string())?;
        if !st.success() {
            return Err("Could not open file.".to_string());
        }
        Ok(())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let st = Command::new("xdg-open")
            .arg(target.as_os_str())
            .status()
            .map_err(|e| e.to_string())?;
        if !st.success() {
            return Err("xdg-open failed.".to_string());
        }
        Ok(())
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = target;
        Err("Opening files is not supported on this platform.".to_string())
    }
}

/// Opens a library file; on macOS, PDFs with `page` use `file://…#page=N` via the `open` command.
#[tauri::command]
pub fn open_library_source(path: String, page: Option<u32>) -> Result<(), String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("Path is empty.".to_string());
    }
    let p = Path::new(trimmed);
    if !p.exists() {
        return Err(format!("Path does not exist: {trimmed}"));
    }
    let is_pdf = p
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("pdf"))
        .unwrap_or(false);

    #[cfg(target_os = "macos")]
    {
        if is_pdf {
            if let Some(pg) = page {
                let canon = p.canonicalize().map_err(|e| e.to_string())?;
                let uri = file_url_with_page(&canon, pg)?;
                let st = Command::new("open")
                    .arg(&uri)
                    .status()
                    .map_err(|e| e.to_string())?;
                if !st.success() {
                    return Err("macOS open failed.".to_string());
                }
                return Ok(());
            }
        }
        let target = p.canonicalize().map_err(|e| e.to_string())?;
        let st = Command::new("open")
            .arg(target.as_os_str())
            .status()
            .map_err(|e| e.to_string())?;
        if !st.success() {
            return Err("macOS open failed.".to_string());
        }
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    {
        let target = p.canonicalize().map_err(|e| e.to_string())?;
        open_non_macos(&target)
    }
}
