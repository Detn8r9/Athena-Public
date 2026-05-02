//! Optional integration with `kiwix-search` / `kiwix-serve` CLI tools (GPL-3).
//! Binaries are never executed from untrusted paths without validation.

use std::path::Path;
use std::process::Command;

fn looks_safe_tool_path(p: &Path) -> bool {
    let s = p.to_string_lossy();
    !s.contains('\0')
        && !s.contains(';')
        && !s.contains('|')
        && !s.contains('`')
        && !s.contains('$')
        && p.is_file()
}

/// Run `kiwix-search ZIM QUERY` and return stdout lines (best-effort).
pub fn kiwix_search_lines(
    binary: &str,
    zim_path: &str,
    query: &str,
) -> Result<Vec<String>, String> {
    let bin = Path::new(binary.trim());
    let zim = Path::new(zim_path.trim());
    if !looks_safe_tool_path(bin) {
        return Err("Invalid kiwix-search binary path.".to_string());
    }
    if !zim.is_file() {
        return Err("ZIM file not found.".to_string());
    }
    let q = query.trim();
    if q.is_empty() {
        return Err("Empty search query.".to_string());
    }
    let out = Command::new(bin)
        .arg(zim)
        .arg(q)
        .output()
        .map_err(|e| format!("Failed to run kiwix-search: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "kiwix-search failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .take(200)
        .collect())
}
