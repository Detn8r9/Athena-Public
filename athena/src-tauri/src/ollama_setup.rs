//! Download or copy the official Ollama CLI into app data and resolve launch paths.

use crate::config::AppConfig;
use crate::ollama::is_ollama_running;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

const GITHUB_LATEST: &str = "https://api.github.com/repos/ollama/ollama/releases/latest";
const USER_AGENT: &str = "Athena-Desktop/0.1";

/// Subdirectory under app data for an Athena-managed `ollama` binary (separate from the user's install).
pub fn managed_ollama_dir(app_data: &Path) -> PathBuf {
    app_data.join("ollama_athena")
}

pub fn managed_ollama_binary_path(app_data: &Path) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        managed_ollama_dir(app_data).join("ollama.exe")
    }
    #[cfg(not(target_os = "windows"))]
    {
        managed_ollama_dir(app_data).join("ollama")
    }
}

fn cli_filename() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "ollama.exe"
    }
    #[cfg(not(target_os = "windows"))]
    {
        "ollama"
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct OllamaSetupStatus {
    pub http_reachable: bool,
    pub sidecar_child_active: bool,
    pub managed_binary_exists: bool,
    pub managed_binary_path: String,
    pub configured_cli_path: Option<String>,
    pub detected_ollama_paths: Vec<String>,
    /// When set, `ollama` was found outside typical install locations — UI may offer to copy into Athena's folder.
    pub unconventional_ollama_path: Option<String>,
}

pub fn build_setup_status(
    app_data: &Path,
    config: &AppConfig,
    sidecar_child_active: bool,
) -> OllamaSetupStatus {
    let managed = managed_ollama_binary_path(app_data);
    let managed_binary_exists = managed.is_file();
    let managed_binary_path = managed.to_string_lossy().to_string();

    let mut detected: Vec<String> = Vec::new();
    for p in static_probe_paths() {
        if Path::new(p).is_file() {
            detected.push(p.to_string());
        }
    }
    if let Ok(w) = which::which("ollama") {
        let s = w.to_string_lossy().to_string();
        if !detected.contains(&s) {
            detected.push(s);
        }
    }

    let unconventional_ollama_path = which::which("ollama")
        .ok()
        .filter(|p| p.is_file())
        .filter(|p| !is_standard_ollama_install_location(p.as_path(), app_data))
        .map(|p| p.to_string_lossy().to_string());

    OllamaSetupStatus {
        http_reachable: is_ollama_running(),
        sidecar_child_active,
        managed_binary_exists,
        managed_binary_path,
        configured_cli_path: config.ollama_athena_cli_path.clone(),
        detected_ollama_paths: detected,
        unconventional_ollama_path,
    }
}

fn static_probe_paths() -> Vec<&'static str> {
    #[cfg(target_os = "macos")]
    {
        vec!["/opt/homebrew/bin/ollama", "/usr/local/bin/ollama"]
    }
    #[cfg(target_os = "windows")]
    {
        vec![]
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        vec!["/usr/bin/ollama", "/usr/local/bin/ollama"]
    }
}

/// Homebrew / system dirs, official Windows install roots, or Athena-managed copy.
pub fn is_standard_ollama_install_location(path: &Path, app_data: &Path) -> bool {
    let lossy = path.to_string_lossy();
    let managed_dir = managed_ollama_dir(app_data);
    let managed_prefix = managed_dir.to_string_lossy();
    if lossy.starts_with(managed_prefix.as_ref()) {
        return true;
    }
    #[cfg(target_os = "macos")]
    {
        if lossy == "/opt/homebrew/bin/ollama" || lossy == "/usr/local/bin/ollama" {
            return true;
        }
    }
    #[cfg(target_os = "linux")]
    {
        if lossy == "/usr/bin/ollama" || lossy == "/usr/local/bin/ollama" {
            return true;
        }
    }
    #[cfg(target_os = "windows")]
    {
        let lower = lossy.to_lowercase();
        if lower.contains("program files\\ollama") || lower.contains("program files (x86)\\ollama")
        {
            return true;
        }
        if lower.contains("local\\programs\\ollama") {
            return true;
        }
    }
    false
}

pub fn resolve_launch_paths(app_data: &Path, config: &AppConfig) -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(ref p) = config.ollama_athena_cli_path {
        let pb = PathBuf::from(p.trim());
        if pb.is_file() {
            v.push(pb);
        }
    }
    let managed = managed_ollama_binary_path(app_data);
    if managed.is_file() {
        v.push(managed);
    }
    v
}

#[derive(Clone, Copy)]
enum ArtifactKind {
    TarGz,
    TarZst,
    Zip,
}

fn platform_artifact() -> Result<(&'static str, ArtifactKind), String> {
    if cfg!(target_os = "macos") {
        return Ok(("ollama-darwin.tgz", ArtifactKind::TarGz));
    }
    if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        return Ok(("ollama-linux-amd64.tar.zst", ArtifactKind::TarZst));
    }
    if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        return Ok(("ollama-linux-arm64.tar.zst", ArtifactKind::TarZst));
    }
    if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        return Ok(("ollama-windows-amd64.zip", ArtifactKind::Zip));
    }
    if cfg!(all(target_os = "windows", target_arch = "aarch64")) {
        return Ok(("ollama-windows-arm64.zip", ArtifactKind::Zip));
    }
    Err("This platform is not supported for automatic Ollama download.".to_string())
}

fn fetch_latest_ollama_tag() -> Result<String, String> {
    if let Ok(t) = std::env::var("ATHENA_OLLAMA_RELEASE_TAG") {
        let t = t.trim().to_string();
        if !t.is_empty() {
            return Ok(t);
        }
    }
    let resp = ureq::get(GITHUB_LATEST)
        .set("User-Agent", USER_AGENT)
        .call()
        .map_err(|e| format!("GitHub API request failed: {e}"))?;
    let v: serde_json::Value = resp
        .into_json()
        .map_err(|e| format!("GitHub API JSON: {e}"))?;
    v.get("tag_name")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "GitHub release missing tag_name.".to_string())
}

fn download_to_file(url: &str, dest: &Path) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let resp = ureq::get(url)
        .set("User-Agent", USER_AGENT)
        .call()
        .map_err(|e| format!("Download failed: {e}"))?;
    let mut file = fs::File::create(dest).map_err(|e| e.to_string())?;
    std::io::copy(&mut resp.into_reader(), &mut file).map_err(|e| e.to_string())?;
    Ok(())
}

fn unpack_tar_gz(path: &Path, out_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;
    let f = fs::File::open(path).map_err(|e| e.to_string())?;
    let gz = flate2::read::GzDecoder::new(f);
    let mut arch = tar::Archive::new(gz);
    arch.unpack(out_dir).map_err(|e| e.to_string())
}

fn unpack_tar_zst(path: &Path, out_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;
    let f = fs::File::open(path).map_err(|e| e.to_string())?;
    let dec = zstd::Decoder::new(f).map_err(|e| e.to_string())?;
    let mut arch = tar::Archive::new(dec);
    arch.unpack(out_dir).map_err(|e| e.to_string())
}

fn unpack_zip(path: &Path, out_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    for i in 0..archive.len() {
        let mut zf = archive.by_index(i).map_err(|e| e.to_string())?;
        let Some(enclosed) = zf.enclosed_name() else {
            continue;
        };
        let out = out_dir.join(enclosed);
        if zf.is_dir() {
            fs::create_dir_all(&out).map_err(|e| e.to_string())?;
        } else {
            if let Some(p) = out.parent() {
                fs::create_dir_all(p).map_err(|e| e.to_string())?;
            }
            let mut dest = fs::File::create(&out).map_err(|e| e.to_string())?;
            std::io::copy(&mut zf, &mut dest).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn find_cli_in_tree(root: &Path) -> Option<PathBuf> {
    let want = cli_filename();
    for e in WalkDir::new(root).max_depth(8).into_iter().flatten() {
        if !e.file_type().is_file() {
            continue;
        }
        if e.path().file_name()?.to_str()? == want {
            return Some(e.path().to_path_buf());
        }
    }
    None
}

fn apply_executable_bit(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path).map_err(|e| e.to_string())?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).map_err(|e| e.to_string())?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

/// Copy `source` (an `ollama` / `ollama.exe` file) into the managed Athena directory.
pub fn copy_cli_into_managed_dir(source: &Path, app_data: &Path) -> Result<PathBuf, String> {
    if !source.is_file() {
        return Err("Source path is not a file.".to_string());
    }
    let name = source
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(cli_filename());
    if !name.eq_ignore_ascii_case(cli_filename()) && !name.eq_ignore_ascii_case("ollama") {
        return Err("Expected an ollama or ollama.exe binary.".to_string());
    }
    let dir = managed_ollama_dir(app_data);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let dest = managed_ollama_binary_path(app_data);
    fs::copy(source, &dest).map_err(|e| e.to_string())?;
    apply_executable_bit(&dest)?;
    Ok(dest)
}

/// Download the official Ollama CLI for this platform from GitHub releases into app data.
pub fn install_ollama_cli_to_app_data(app_data: &Path) -> Result<PathBuf, String> {
    let (filename, kind) = platform_artifact()?;
    let tag = fetch_latest_ollama_tag()?;
    let url = format!(
        "https://github.com/ollama/ollama/releases/download/{}/{}",
        tag, filename
    );

    let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
    let archive_path = tmp.path().join(filename);
    download_to_file(&url, &archive_path)?;

    let extract_dir = tmp.path().join("extract");
    match kind {
        ArtifactKind::TarGz => unpack_tar_gz(&archive_path, &extract_dir)?,
        ArtifactKind::TarZst => unpack_tar_zst(&archive_path, &extract_dir)?,
        ArtifactKind::Zip => unpack_zip(&archive_path, &extract_dir)?,
    }

    let found = find_cli_in_tree(&extract_dir)
        .ok_or_else(|| "Downloaded archive did not contain an ollama binary.".to_string())?;

    let dir = managed_ollama_dir(app_data);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let dest = managed_ollama_binary_path(app_data);
    fs::copy(&found, &dest).map_err(|e| e.to_string())?;
    apply_executable_bit(&dest)?;
    Ok(dest)
}
