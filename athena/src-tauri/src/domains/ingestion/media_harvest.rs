//! WAN / local document harvest for the Documents & Media hub.
//! Heavy work runs in `spawn_blocking` from async commands.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;
use tauri::Emitter;

pub const HARVEST_PROGRESS_EVENT: &str = "athena:harvest-progress";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HarvestProgressPayload {
    pub harvest_id: String,
    pub phase: String,
    pub message: Option<String>,
    pub target_path: Option<String>,
    /// First matching `.srt` / `.vtt` next to the downloaded video (same stem), for transcript ingest.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub companion_transcript_path: Option<String>,
}

fn emit(
    app: &tauri::AppHandle,
    harvest_id: &str,
    phase: &str,
    message: Option<String>,
    target_path: Option<String>,
    companion_transcript_path: Option<String>,
) {
    let _ = app.emit(
        HARVEST_PROGRESS_EVENT,
        HarvestProgressPayload {
            harvest_id: harvest_id.to_string(),
            phase: phase.to_string(),
            message,
            target_path,
            companion_transcript_path,
        },
    );
}

#[derive(Debug, serde::Serialize)]
struct AthenaSidecar {
    original_url: String,
    creator_name: Option<String>,
    harvest_timestamp: i64,
    media_type: String,
}

fn write_sidecar(dir: &Path, stem: &str, sidecar: &AthenaSidecar) -> Result<(), String> {
    let p = dir.join(format!("{stem}.athena.json"));
    let json = serde_json::to_string_pretty(sidecar).map_err(|e| e.to_string())?;
    fs::write(&p, json).map_err(|e| format!("{}: {e}", p.display()))
}

fn is_video_host_url(url: &str) -> bool {
    let u = url.trim().to_ascii_lowercase();
    u.contains("youtube.com/")
        || u.contains("youtu.be/")
        || u.contains("m.youtube.com")
        || u.contains("vimeo.com/")
}

fn resolve_yt_dlp() -> PathBuf {
    if let Ok(p) = std::env::var("YT_DLP_PATH") {
        let pb = PathBuf::from(p.trim());
        if pb.as_os_str().len() > 0 {
            return pb;
        }
    }
    PathBuf::from("yt-dlp")
}

/// Parse optional uploader from yt-dlp `--write-info-json` companion file.
fn companion_subtitle_path(dir: &Path, stem: &str) -> Option<String> {
    let candidates = [
        format!("{stem}.en.srt"),
        format!("{stem}.srt"),
        format!("{stem}.en.vtt"),
        format!("{stem}.vtt"),
    ];
    for n in candidates {
        let p = dir.join(&n);
        if p.is_file() {
            return Some(p.to_string_lossy().to_string());
        }
    }
    let rd = fs::read_dir(dir).ok()?;
    for ent in rd.flatten() {
        let p = ent.path();
        let name = p.file_name()?.to_string_lossy().to_string();
        if p.is_file()
            && name.starts_with(stem)
            && (name.ends_with(".srt") || name.ends_with(".vtt"))
        {
            return Some(p.to_string_lossy().to_string());
        }
    }
    None
}

fn read_creator_from_info_json(dir: &Path, video_id_hint: &str) -> Option<String> {
    let read_dir = fs::read_dir(dir).ok()?;
    for ent in read_dir.flatten() {
        let p = ent.path();
        let name = p.file_name()?.to_string_lossy().to_string();
        if !name.ends_with(".info.json") {
            continue;
        }
        let raw = fs::read_to_string(&p).ok()?;
        if video_id_hint.is_empty() || raw.contains(video_id_hint) || name.contains(video_id_hint) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
                return v
                    .get("uploader")
                    .or_else(|| v.get("channel"))
                    .and_then(|x| x.as_str())
                    .map(|s| s.to_string());
            }
        }
    }
    None
}

fn harvest_youtube_blocking(
    app: &tauri::AppHandle,
    harvest_id: &str,
    url: &str,
    target_dir: &Path,
) -> Result<(), String> {
    fs::create_dir_all(target_dir).map_err(|e| e.to_string())?;
    emit(
        app,
        harvest_id,
        "downloading",
        Some("Running yt-dlp (video ≤720p + subtitles)…".into()),
        None,
        None,
    );

    let ytdlp = resolve_yt_dlp();
    let out_tpl = target_dir.join("%(id)s.%(ext)s");
    let out_arg = out_tpl.to_string_lossy().to_string();

    let status = Command::new(&ytdlp)
        .current_dir(target_dir)
        .arg("--no-playlist")
        .arg("-f")
        .arg("bv*[height<=720]+ba/b[height<=720]/bv*+ba/b")
        .arg("--merge-output-format")
        .arg("mp4")
        .arg("--write-subs")
        .arg("--write-auto-subs")
        .arg("--sub-langs")
        .arg("en.*,all")
        .arg("--convert-subs")
        .arg("srt")
        .arg("--write-info-json")
        .arg("-o")
        .arg(&out_arg)
        .arg(url)
        .status()
        .map_err(|e| {
            format!(
                "yt-dlp failed to start ({e}). Install yt-dlp and ensure it is on PATH, or set YT_DLP_PATH."
            )
        })?;

    if !status.success() {
        return Err("yt-dlp exited with an error. Check the URL and network.".to_string());
    }

    let mut video_stem: Option<String> = None;
    for ent in fs::read_dir(target_dir).map_err(|e| e.to_string())? {
        let p = ent.map_err(|e| e.to_string())?.path();
        if p.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("mp4")) {
            video_stem = p.file_stem().and_then(|s| s.to_str()).map(|s| s.to_string());
            break;
        }
    }
    let stem = video_stem.unwrap_or_else(|| "video".to_string());
    let creator = read_creator_from_info_json(target_dir, &stem);
    let now = chrono::Utc::now().timestamp();
    let sidecar = AthenaSidecar {
        original_url: url.to_string(),
        creator_name: creator,
        harvest_timestamp: now,
        media_type: "video".into(),
    };
    write_sidecar(target_dir, &stem, &sidecar)?;

    let companion = companion_subtitle_path(target_dir, &stem);
    emit(
        app,
        harvest_id,
        "complete",
        Some("Download finished.".into()),
        Some(target_dir.join(format!("{stem}.mp4")).to_string_lossy().into()),
        companion.clone(),
    );
    Ok(())
}

fn harvest_generic_http_blocking(
    app: &tauri::AppHandle,
    harvest_id: &str,
    url: &str,
    target_dir: &Path,
) -> Result<(), String> {
    fs::create_dir_all(target_dir).map_err(|e| e.to_string())?;
    emit(
        app,
        harvest_id,
        "downloading",
        Some("Fetching URL…".into()),
        None,
        None,
    );

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())?;
    let mut resp = client.get(url).send().map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let fname = url
        .rsplit('/')
        .next()
        .and_then(|s| {
            let t = s.trim();
            if t.is_empty() || t.contains('?') {
                None
            } else {
                Some(t.to_string())
            }
        })
        .unwrap_or_else(|| "download.bin".into());
    let safe: String = fname
        .chars()
        .map(|c| if matches!(c, '/' | '\\' | ':' | '<' | '>' | '|' | '?' | '*' | '\0') { '_' } else { c })
        .collect();
    let dest = crate::library_ingest::dedupe_destination_path(target_dir, &safe);
    let mut f = fs::File::create(&dest).map_err(|e| e.to_string())?;
    std::io::copy(&mut resp, &mut f).map_err(|e| e.to_string())?;

    let stem = dest
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("download")
        .to_string();
    let now = chrono::Utc::now().timestamp();
    let sidecar = AthenaSidecar {
        original_url: url.to_string(),
        creator_name: None,
        harvest_timestamp: now,
        media_type: "web".into(),
    };
    write_sidecar(target_dir, &stem, &sidecar)?;

    emit(
        app,
        harvest_id,
        "complete",
        Some("Saved.".into()),
        Some(dest.to_string_lossy().into()),
        None,
    );
    Ok(())
}

fn harvest_media_url_inner(app: &tauri::AppHandle, harvest_id: &str, url: &str, target_dir: &Path) -> Result<(), String> {
    emit(
        app,
        harvest_id,
        "pending",
        Some("Starting harvest…".into()),
        None,
        None,
    );
    if is_video_host_url(url) {
        harvest_youtube_blocking(app, harvest_id, url, target_dir)
    } else {
        harvest_generic_http_blocking(app, harvest_id, url, target_dir)
    }
}

#[tauri::command]
pub async fn harvest_media_url(
    url: String,
    target_dir: String,
    correlation_id: Option<String>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let url = url.trim().to_string();
    if url.is_empty() {
        return Err("URL is required.".into());
    }
    let target_dir = target_dir.trim().to_string();
    if target_dir.is_empty() {
        return Err("target_dir is required.".into());
    }
    let hid = correlation_id
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let app_c = app.clone();
    let url_c = url.clone();
    let dir = PathBuf::from(target_dir);
    let hid_c = hid.clone();
    tokio::task::spawn_blocking(move || {
        let r = harvest_media_url_inner(&app_c, &hid_c, &url_c, &dir);
        if let Err(ref e) = r {
            emit(
                &app_c,
                &hid_c,
                "error",
                Some(e.clone()),
                None,
                None,
            );
        }
        r
    })
    .await
    .map_err(|e| format!("harvest task join: {e}"))?
}

/// Copy local files into `target_dir`, write `.athena.json` sidecars, intended for follow-up `ingest_local_document`.
#[tauri::command]
pub async fn harvest_local_documents(
    paths: Vec<String>,
    target_dir: String,
    correlation_id: Option<String>,
    app: tauri::AppHandle,
) -> Result<Vec<String>, String> {
    if paths.is_empty() {
        return Ok(vec![]);
    }
    let target_dir = PathBuf::from(target_dir.trim());
    if target_dir.as_os_str().is_empty() {
        return Err("target_dir is required.".into());
    }
    let hid = correlation_id
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let app_c = app.clone();
    let paths_c = paths.clone();
    let hid_c = hid.clone();
    let join = tokio::task::spawn_blocking(move || -> Result<Vec<String>, String> {
        fs::create_dir_all(&target_dir).map_err(|e| e.to_string())?;
        emit(
            &app_c,
            &hid_c,
            "downloading",
            Some(format!("Copying {} file(s)…", paths_c.len())),
            None,
            None,
        );
        let mut out_paths = Vec::new();
        for p in paths_c {
            let src = PathBuf::from(p.trim());
            if !src.is_file() {
                continue;
            }
            let fname = src
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| format!("Invalid file name: {}", src.display()))?;
            let dest = crate::library_ingest::dedupe_destination_path(&target_dir, fname);
            fs::copy(&src, &dest).map_err(|e| format!("{} -> {}: {e}", src.display(), dest.display()))?;
            let stem = dest
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("document")
                .to_string();
            let ext = dest
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            let media_type = if ext == "pdf" {
                "pdf"
            } else if ext == "doc" || ext == "docx" {
                "word"
            } else {
                "document"
            };
            let sidecar = AthenaSidecar {
                original_url: format!("file://{}", src.display()),
                creator_name: None,
                harvest_timestamp: chrono::Utc::now().timestamp(),
                media_type: media_type.into(),
            };
            write_sidecar(&target_dir, &stem, &sidecar)?;
            out_paths.push(dest.to_string_lossy().to_string());
        }
        emit(
            &app_c,
            &hid_c,
            "complete",
            Some("Local files staged.".into()),
            None,
            None,
        );
        Ok(out_paths)
    })
    .await;

    match join {
        Ok(Ok(paths)) => Ok(paths),
        Ok(Err(e)) => {
            emit(&app, &hid, "error", Some(e.clone()), None, None);
            Err(e)
        }
        Err(e) => Err(format!("harvest_local task: {e}")),
    }
}

#[tauri::command]
pub async fn resolve_media_harvest_dir(app: tauri::AppHandle) -> Result<String, String> {
    let base = crate::portable::resolved_app_data_dir(&app)?;
    let dir = crate::athena_paths::AthenaPaths::new(&base)
        .user_downloads_dir()
        .join("documents")
        .join("harvest_staging");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.to_string_lossy().to_string())
}
