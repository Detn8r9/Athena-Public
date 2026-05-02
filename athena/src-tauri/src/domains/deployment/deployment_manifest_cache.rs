//! Local cache of HEAD-derived sizes and weak validators for deployment downloads.
//! Stored at `<AthenaData>/.athena/deployment_manifest_cache.json`.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const RELATIVE_PATH: &str = ".athena/deployment_manifest_cache.json";

pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestUrlEntry {
    pub expected_bytes: u64,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub last_head_at_unix: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestCacheFile {
    #[serde(default = "default_format_version")]
    pub format_version: u32,
    #[serde(default)]
    pub entries: HashMap<String, ManifestUrlEntry>,
}

fn default_format_version() -> u32 {
    FORMAT_VERSION
}

impl Default for ManifestCacheFile {
    fn default() -> Self {
        Self {
            format_version: FORMAT_VERSION,
            entries: HashMap::new(),
        }
    }
}

pub fn manifest_cache_path(athena_root: &Path) -> PathBuf {
    athena_root.join(RELATIVE_PATH)
}

pub fn normalize_url_key(url: &str) -> String {
    url.trim().to_string()
}

pub async fn load_manifest_cache(path: &Path) -> Result<ManifestCacheFile, String> {
    if !tokio::fs::try_exists(path)
        .await
        .map_err(|e| format!("stat manifest cache {}: {e}", path.display()))?
    {
        return Ok(ManifestCacheFile::default());
    }
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|e| format!("read manifest cache {}: {e}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("parse manifest cache: {e}"))
}

pub async fn save_manifest_cache(path: &Path, data: &ManifestCacheFile) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    let buf = serde_json::to_vec_pretty(data).map_err(|e| e.to_string())?;

    // Atomic replace: write temp, flush + sync, rename.
    let mut f = tokio::fs::File::create(&tmp).await.map_err(|e| e.to_string())?;
    use tokio::io::AsyncWriteExt;
    f.write_all(&buf).await.map_err(|e| e.to_string())?;
    f.flush().await.map_err(|e| e.to_string())?;
    f.sync_all().await.map_err(|e| e.to_string())?;
    drop(f);

    tokio::fs::rename(&tmp, path).await.map_err(|e| e.to_string())?;
    Ok(())
}

/// When `Content-Length` from HEAD disagrees with the manifest, persist the server size as the new expectation.
pub async fn persist_head_size_if_changed(
    athena_root: &Path,
    url_key: &str,
    manifest_expected: u64,
    content_length: u64,
    etag: Option<String>,
    last_modified: Option<String>,
) -> Result<(), String> {
    if content_length == manifest_expected {
        return Ok(());
    }
    let path = manifest_cache_path(athena_root);
    let mut file = load_manifest_cache(&path).await?;
    file.format_version = FORMAT_VERSION;
    file.entries.insert(
        url_key.to_string(),
        ManifestUrlEntry {
            expected_bytes: content_length,
            etag,
            last_modified,
            last_head_at_unix: Utc::now().timestamp(),
        },
    );
    save_manifest_cache(&path, &file).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persist_changed_size_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let url = "https://example.com/a.bin";
        let key = normalize_url_key(url);
        tauri::async_runtime::block_on(async {
            persist_head_size_if_changed(root, &key, 100, 200, None, None)
                .await
                .unwrap();
        });
        let path = manifest_cache_path(root);
        let c = tauri::async_runtime::block_on(async { load_manifest_cache(&path).await.unwrap() });
        assert_eq!(c.entries.get(&key).unwrap().expected_bytes, 200);
    }
}
