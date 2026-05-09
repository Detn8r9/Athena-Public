use std::collections::HashMap;
use std::sync::Arc;

use tauri::Emitter;
use tokio_util::sync::CancellationToken;

use crate::config::{load_config, save_config, LibraryInstallerLocation};
use crate::package_manager::{self, LibraryManifest, ModelManifest};
use crate::AppState;

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadPackageArgs {
    manifest_type: String,
    download_url: String,
    destination_folder: String,
    download_id: String,
    file_name: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelPackageDownloadArgs {
    manifest_type: String,
    download_id: String,
}

struct PackageDownloadEntryGuard {
    map: Arc<std::sync::Mutex<HashMap<String, CancellationToken>>>,
    key: String,
}

impl Drop for PackageDownloadEntryGuard {
    fn drop(&mut self) {
        if let Ok(mut g) = self.map.lock() {
            g.remove(&self.key);
        }
    }
}

#[tauri::command]
pub fn get_library_manifest(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<LibraryManifest, String> {
    let _guard = state.config_lock.lock().map_err(|e| e.to_string())?;
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let cfg = load_config(&app_data_dir);
    drop(_guard);
    package_manager::load_library_manifest_with_location(cfg.library_installer_location.as_ref())
}

#[tauri::command]
pub fn get_library_installer_location(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<Option<LibraryInstallerLocation>, String> {
    let _guard = state.config_lock.lock().map_err(|e| e.to_string())?;
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    Ok(load_config(&app_data_dir).library_installer_location)
}

#[tauri::command]
pub fn save_library_installer_location_nyc_default(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<LibraryInstallerLocation, String> {
    let loc = crate::library_installer_location::default_nyc_location();
    let _guard = state.config_lock.lock().map_err(|e| e.to_string())?;
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let mut cfg = load_config(&app_data_dir);
    cfg.library_installer_location = Some(loc.clone());
    save_config(&app_data_dir, &cfg)?;
    Ok(loc)
}

#[tauri::command]
pub async fn save_library_installer_location_geocode(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    query: String,
) -> Result<LibraryInstallerLocation, String> {
    let loc = crate::library_installer_location::geocode_us_place(&query).await?;
    let _guard = state.config_lock.lock().map_err(|e| e.to_string())?;
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let mut cfg = load_config(&app_data_dir);
    cfg.library_installer_location = Some(loc.clone());
    save_config(&app_data_dir, &cfg)?;
    Ok(loc)
}

#[tauri::command]
pub fn clear_library_installer_location(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let _guard = state.config_lock.lock().map_err(|e| e.to_string())?;
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let mut cfg = load_config(&app_data_dir);
    cfg.library_installer_location = None;
    save_config(&app_data_dir, &cfg)
}

#[tauri::command]
pub fn get_model_manifest() -> Result<ModelManifest, String> {
    package_manager::load_model_manifest_bundled()
}

#[tauri::command]
pub fn get_models_dir(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<Option<String>, String> {
    let _guard = state.config_lock.lock().map_err(|e| e.to_string())?;
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    Ok(load_config(&app_data_dir).models_dir)
}

#[tauri::command]
pub fn set_models_dir(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    value: Option<String>,
) -> Result<(), String> {
    let _guard = state.config_lock.lock().map_err(|e| e.to_string())?;
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let mut cfg = load_config(&app_data_dir);
    cfg.models_dir = value.and_then(|s| {
        let t = s.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    });
    save_config(&app_data_dir, &cfg)
}

#[tauri::command]
pub fn cancel_package_download(
    state: tauri::State<'_, AppState>,
    args: CancelPackageDownloadArgs,
) -> Result<(), String> {
    let manifest_type = args.manifest_type.trim().to_lowercase();
    if manifest_type != "library" && manifest_type != "models" {
        return Err("manifest_type must be 'library' or 'models'.".to_string());
    }
    let download_id = args.download_id.trim().to_string();
    if download_id.is_empty() {
        return Err("download_id is required.".to_string());
    }
    let key = format!("{manifest_type}:{download_id}");
    let g = state
        .active_package_downloads
        .lock()
        .map_err(|_| "Package download state lock was poisoned.".to_string())?;
    let Some(token) = g.get(&key) else {
        return Err("No active download for that id.".to_string());
    };
    token.cancel();
    Ok(())
}

#[tauri::command]
pub async fn download_package(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    args: DownloadPackageArgs,
) -> Result<(), String> {
    let manifest_type = args.manifest_type.trim().to_lowercase();
    if manifest_type != "library" && manifest_type != "models" {
        return Err("manifest_type must be 'library' or 'models'.".to_string());
    }
    let download_url = args.download_url.trim().to_string();
    if download_url.is_empty() {
        return Err("download_url is required.".to_string());
    }
    let parsed =
        reqwest::Url::parse(&download_url).map_err(|e| format!("Invalid download URL: {e}"))?;
    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return Err("Download URL must use http or https.".to_string());
    }
    let destination_folder = args.destination_folder.trim().to_string();
    if destination_folder.is_empty() {
        return Err("destination_folder is required.".to_string());
    }
    let download_id = args.download_id.trim().to_string();
    if download_id.is_empty() {
        return Err("download_id is required.".to_string());
    }
    let file_name = args.file_name.trim().to_string();
    if file_name.is_empty() {
        return Err("file_name is required.".to_string());
    }

    let dest_dir = std::path::PathBuf::from(destination_folder);
    std::fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;
    let dest_dir = std::fs::canonicalize(&dest_dir).map_err(|e| {
        format!(
            "Could not resolve absolute path for destination folder {}: {e}",
            dest_dir.display()
        )
    })?;
    let (final_path, part_path) = package_manager::destination_paths(&dest_dir, &file_name)?;

    let cancel = CancellationToken::new();
    let run_cancel = cancel.clone();

    {
        let mut active_guard = state
            .active_package_downloads
            .lock()
            .map_err(|_| "Package download state lock was poisoned.".to_string())?;
        let key = format!("{manifest_type}:{download_id}");
        if active_guard.contains_key(&key) {
            return Err("This download is already in progress.".to_string());
        }
        active_guard.insert(key, cancel);
    }

    let app_handle = app.clone();
    let active_map = Arc::clone(&state.active_package_downloads);
    let key_remove = format!("{manifest_type}:{download_id}");
    let download_id_task = download_id.clone();
    let manifest_type_task = manifest_type.clone();
    tauri::async_runtime::spawn(async move {
        let _guard = PackageDownloadEntryGuard {
            map: active_map,
            key: key_remove,
        };
        let outcome = package_manager::stream_download_to_file(
            app_handle.clone(),
            manifest_type_task.clone(),
            download_id_task.clone(),
            download_url,
            final_path.clone(),
            part_path.clone(),
            run_cancel,
        )
        .await;

        match outcome {
            Ok(()) => {
                let path_string = final_path.to_string_lossy().to_string();
                let _ = app_handle.emit(
                    package_manager::PACKAGE_DOWNLOAD_COMPLETE_EVENT,
                    package_manager::PackageDownloadCompletePayload {
                        manifest_type: manifest_type_task.clone(),
                        download_id: download_id_task.clone(),
                        path: path_string,
                    },
                );
            }
            Err(ref msg)
                if msg.as_str() == package_manager::PACKAGE_DOWNLOAD_CANCELLED_MSG =>
            {
                let _ = app_handle.emit(
                    package_manager::PACKAGE_DOWNLOAD_CANCELLED_EVENT,
                    package_manager::PackageDownloadCancelledPayload {
                        manifest_type: manifest_type_task.clone(),
                        download_id: download_id_task.clone(),
                    },
                );
            }
            Err(message) => {
                let _ = std::fs::remove_file(&part_path);
                let _ = app_handle.emit(
                    package_manager::PACKAGE_DOWNLOAD_ERROR_EVENT,
                    package_manager::PackageDownloadErrorPayload {
                        manifest_type: manifest_type_task,
                        download_id: download_id_task,
                        error: message,
                    },
                );
            }
        }
    });

    Ok(())
}
