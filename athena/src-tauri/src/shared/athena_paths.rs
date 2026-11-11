//! AthenaData layout v2: nested `app/` (bundled defaults) + `user/` (installed data).
//!
//! The Athena root is still whatever `portable::resolved_app_data_dir` returns.
//! Call [`ensure_layout`] once at startup before opening the DB or writing config.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const LAYOUT_FILENAME: &str = "athena_layout.json";
pub const LAYOUT_VERSION: u32 = 2;

pub const DB_FILENAME: &str = "athena_index.db";
pub const CONFIG_FILENAME: &str = "athena_config.json";
pub const MARKETPLACE_STATE_FILENAME: &str = "marketplace_state.json";

/// Tag for the bundled offline default instruct model (small; Conversation-friendly).
pub const BUNDLED_DEFAULT_OLLAMA_MODEL: &str = "llama3.2:1b";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutFile {
    pub version: u32,
}

#[derive(Debug, Clone)]
pub struct AthenaPaths {
    pub root: PathBuf,
}

impl AthenaPaths {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn app_dir(&self) -> PathBuf {
        self.root.join("app")
    }

    pub fn user_dir(&self) -> PathBuf {
        self.root.join("user")
    }

    pub fn layout_path(&self) -> PathBuf {
        self.root.join(LAYOUT_FILENAME)
    }

    pub fn user_config_dir(&self) -> PathBuf {
        self.user_dir().join("config")
    }

    pub fn user_db_dir(&self) -> PathBuf {
        self.user_dir().join("db")
    }

    pub fn user_models_dir(&self) -> PathBuf {
        self.user_dir().join("models")
    }

    pub fn user_maps_dir(&self) -> PathBuf {
        self.user_dir().join("maps")
    }

    pub fn user_library_dir(&self) -> PathBuf {
        self.user_dir().join("library")
    }

    pub fn user_apps_dir(&self) -> PathBuf {
        self.user_dir().join("apps")
    }

    pub fn user_archives_dir(&self) -> PathBuf {
        self.user_dir().join("archives")
    }

    pub fn user_downloads_dir(&self) -> PathBuf {
        self.user_dir().join("downloads")
    }

    pub fn user_chats_dir(&self) -> PathBuf {
        self.user_dir().join("chats")
    }

    pub fn app_models_dir(&self) -> PathBuf {
        self.app_dir().join("models")
    }

    pub fn bundled_ollama_models_dir(&self) -> PathBuf {
        self.app_models_dir().join("ollama")
    }

    pub fn user_ollama_models_dir(&self) -> PathBuf {
        self.user_models_dir().join("ollama")
    }

    pub fn user_chat_models_dir(&self) -> PathBuf {
        self.user_models_dir().join("chat")
    }

    pub fn user_vision_models_dir(&self) -> PathBuf {
        self.user_models_dir().join("vision")
    }

    pub fn user_voice_models_dir(&self) -> PathBuf {
        self.user_models_dir().join("voice")
    }

    pub fn user_embeddings_dir(&self) -> PathBuf {
        self.user_models_dir().join("embeddings")
    }

    pub fn user_rerankers_dir(&self) -> PathBuf {
        self.user_models_dir().join("rerankers")
    }

    pub fn app_chat_models_dir(&self) -> PathBuf {
        self.app_models_dir().join("chat")
    }

    pub fn app_embeddings_dir(&self) -> PathBuf {
        self.app_models_dir().join("embeddings")
    }

    pub fn app_rerankers_dir(&self) -> PathBuf {
        self.app_models_dir().join("rerankers")
    }

    /// Writable config path (layout v2).
    pub fn config_path(&self) -> PathBuf {
        self.user_config_dir().join(CONFIG_FILENAME)
    }

    /// Legacy flat config at Athena root.
    pub fn legacy_config_path(&self) -> PathBuf {
        self.root.join(CONFIG_FILENAME)
    }

    pub fn marketplace_state_path(&self) -> PathBuf {
        self.user_config_dir().join(MARKETPLACE_STATE_FILENAME)
    }

    pub fn legacy_marketplace_state_path(&self) -> PathBuf {
        self.root.join(MARKETPLACE_STATE_FILENAME)
    }

    pub fn db_path(&self) -> PathBuf {
        self.user_db_dir().join(DB_FILENAME)
    }

    pub fn legacy_db_path(&self) -> PathBuf {
        self.root.join(DB_FILENAME)
    }

    /// Prefer user DB; fall back to legacy flat path if not yet migrated.
    pub fn resolve_db_path(&self) -> PathBuf {
        let modern = self.db_path();
        if modern.exists() {
            return modern;
        }
        let legacy = self.legacy_db_path();
        if legacy.exists() {
            return legacy;
        }
        modern
    }

    /// Prefer user config; fall back to legacy root config.
    pub fn resolve_config_path(&self) -> PathBuf {
        let modern = self.config_path();
        if modern.exists() {
            return modern;
        }
        let legacy = self.legacy_config_path();
        if legacy.exists() {
            return legacy;
        }
        modern
    }

    pub fn resolve_marketplace_state_path(&self) -> PathBuf {
        let modern = self.marketplace_state_path();
        if modern.exists() {
            return modern;
        }
        let legacy = self.legacy_marketplace_state_path();
        if legacy.exists() {
            return legacy;
        }
        modern
    }

    pub fn read_layout_version(&self) -> u32 {
        let path = self.layout_path();
        if let Ok(s) = fs::read_to_string(&path) {
            if let Ok(f) = serde_json::from_str::<LayoutFile>(&s) {
                return f.version;
            }
        }
        1
    }

    pub fn write_layout_version(&self, version: u32) -> Result<(), String> {
        let path = self.layout_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let body = LayoutFile { version };
        let s = serde_json::to_string_pretty(&body).map_err(|e| e.to_string())?;
        fs::write(&path, s).map_err(|e| e.to_string())
    }
}

/// Create `app/` + `user/` trees, migrate legacy flat layout once, seed bundled Ollama placeholder.
pub fn ensure_layout(root: &Path) -> Result<AthenaPaths, String> {
    let paths = AthenaPaths::new(root);
    fs::create_dir_all(&paths.root).map_err(|e| e.to_string())?;

    let ver = paths.read_layout_version();
    if ver < LAYOUT_VERSION {
        migrate_legacy_to_user(&paths)?;
        seed_app_tree(&paths)?;
        paths.write_layout_version(LAYOUT_VERSION)?;
    } else {
        ensure_user_skeleton(&paths)?;
        ensure_app_skeleton(&paths)?;
    }
    Ok(paths)
}

fn ensure_user_skeleton(paths: &AthenaPaths) -> Result<(), String> {
    for p in [
        paths.user_config_dir(),
        paths.user_db_dir(),
        paths.user_chat_models_dir(),
        paths.user_vision_models_dir(),
        paths.user_voice_models_dir(),
        paths.user_embeddings_dir(),
        paths.user_rerankers_dir(),
        paths.user_ollama_models_dir(),
        paths.user_maps_dir(),
        paths.user_library_dir(),
        paths.user_apps_dir(),
        paths.user_archives_dir(),
        paths.user_downloads_dir().join("packages"),
        paths.user_downloads_dir().join("maps"),
        paths.user_chats_dir(),
    ] {
        fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn ensure_app_skeleton(paths: &AthenaPaths) -> Result<(), String> {
    for p in [
        paths.bundled_ollama_models_dir(),
        paths.app_chat_models_dir(),
        paths.app_embeddings_dir(),
        paths.app_rerankers_dir(),
    ] {
        fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    }
    let readme = paths.app_dir().join("README.txt");
    if !readme.exists() {
        let _ = fs::write(
            &readme,
            "Athena bundled defaults (Outpost copies this folder).\n\
             Do not place user downloads here — use AthenaData/user/ instead.\n\
             Default Ollama model seed: app/models/ollama/\n",
        );
    }
    let marker = paths
        .bundled_ollama_models_dir()
        .join("ATHENA_DEFAULT_MODEL.txt");
    if !marker.exists() {
        let _ = fs::write(
            &marker,
            format!(
                "{}\n# Place a pre-populated Ollama models directory here for fully offline first boot.\n\
                 # Until then, Athena may seed from this tag via network once, then stay offline.\n",
                BUNDLED_DEFAULT_OLLAMA_MODEL
            ),
        );
    }
    Ok(())
}

fn seed_app_tree(paths: &AthenaPaths) -> Result<(), String> {
    ensure_app_skeleton(paths)?;
    // If legacy already had an ollama-like store we do not move it into app — user owns installed models.
    Ok(())
}

fn migrate_legacy_to_user(paths: &AthenaPaths) -> Result<(), String> {
    ensure_user_skeleton(paths)?;
    ensure_app_skeleton(paths)?;

    // models/ → user/models/ (skip if user/models already populated from a prior partial run)
    let legacy_models = paths.root.join("models");
    if legacy_models.is_dir() && !dir_nonempty(&paths.user_models_dir().join("chat")) {
        // Move whole tree into user/models if user/models is mostly empty.
        let dest = paths.user_models_dir();
        if !dir_nonempty(&dest) {
            move_or_merge_dir(&legacy_models, &dest)?;
        } else {
            merge_dir_contents(&legacy_models, &dest)?;
            let _ = fs::remove_dir_all(&legacy_models);
        }
    }

    move_dir_if_present(paths.root.join("maps"), paths.user_maps_dir())?;
    move_dir_if_present(paths.root.join("downloads"), paths.user_downloads_dir())?;
    move_dir_if_present(paths.root.join("library"), paths.user_library_dir())?;
    move_dir_if_present(paths.root.join("chats"), paths.user_chats_dir())?;
    move_dir_if_present(paths.root.join("apps"), paths.user_apps_dir())?;

    // DB (+ wal/shm)
    for suffix in ["", "-wal", "-shm"] {
        let name = format!("{DB_FILENAME}{suffix}");
        let src = paths.root.join(&name);
        if src.is_file() {
            let dst = paths.user_db_dir().join(&name);
            if !dst.exists() {
                move_file(&src, &dst)?;
            }
        }
    }

    // Config + marketplace state
    for (src_name, dest) in [
        (CONFIG_FILENAME, paths.config_path()),
        (MARKETPLACE_STATE_FILENAME, paths.marketplace_state_path()),
    ] {
        let src = paths.root.join(src_name);
        if src.is_file() && !dest.exists() {
            move_file(&src, &dest)?;
        }
    }

    // Other loose JSON state files commonly at root → user/config
    for name in [
        "athena_lan_identity.json",
        "downloaded_maps.json",
        "map_pois.json",
        "theme_config.json",
        "knowledge_block.json",
    ] {
        let src = paths.root.join(name);
        if src.is_file() {
            let dst = paths.user_config_dir().join(name);
            if !dst.exists() {
                let _ = move_file(&src, &dst);
            }
        }
    }

    Ok(())
}

fn dir_nonempty(p: &Path) -> bool {
    fs::read_dir(p)
        .ok()
        .and_then(|mut d| d.next())
        .is_some()
}

fn move_dir_if_present(src: PathBuf, dst: PathBuf) -> Result<(), String> {
    if !src.is_dir() {
        return Ok(());
    }
    if !dst.exists() {
        return move_or_merge_dir(&src, &dst);
    }
    if !dir_nonempty(&dst) {
        let _ = fs::remove_dir_all(&dst);
        return move_or_merge_dir(&src, &dst);
    }
    merge_dir_contents(&src, &dst)?;
    let _ = fs::remove_dir_all(&src);
    Ok(())
}

fn move_or_merge_dir(src: &Path, dst: &Path) -> Result<(), String> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    match fs::rename(src, dst) {
        Ok(()) => Ok(()),
        Err(_) => {
            // Cross-device: copy then remove
            copy_dir_recursive(src, dst)?;
            fs::remove_dir_all(src).map_err(|e| e.to_string())?;
            Ok(())
        }
    }
}

fn merge_dir_contents(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            if to.exists() {
                merge_dir_contents(&from, &to)?;
            } else {
                move_or_merge_dir(&from, &to)?;
            }
        } else if !to.exists() {
            move_file(&from, &to)?;
        }
    }
    Ok(())
}

fn move_file(src: &Path, dst: &Path) -> Result<(), String> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    match fs::rename(src, dst) {
        Ok(()) => Ok(()),
        Err(_) => {
            fs::copy(src, dst).map_err(|e| e.to_string())?;
            fs::remove_file(src).map_err(|e| e.to_string())?;
            Ok(())
        }
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::copy(&from, &to).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Copy bundled Ollama store from `app/models/ollama` → `user/models/ollama` when user store lacks blobs.
pub fn seed_user_ollama_from_app(paths: &AthenaPaths) -> Result<bool, String> {
    let src = paths.bundled_ollama_models_dir();
    let dst = paths.user_ollama_models_dir();
    fs::create_dir_all(&dst).map_err(|e| e.to_string())?;
    if !ollama_store_has_blobs(&src) {
        return Ok(false);
    }
    if ollama_store_has_blobs(&dst) {
        return Ok(false);
    }
    merge_dir_contents(&src, &dst)?;
    Ok(true)
}

fn ollama_store_has_blobs(dir: &Path) -> bool {
    ollama_store_has_model_blobs(dir)
}

/// True when an Ollama models directory has blobs or manifests (usable offline seed).
pub fn ollama_store_has_model_blobs(dir: &Path) -> bool {
    let blobs = dir.join("blobs");
    let manifests = dir.join("manifests");
    (blobs.is_dir() && dir_nonempty(&blobs)) || (manifests.is_dir() && dir_nonempty(&manifests))
}

/// Read default model tag from app marker, else [`BUNDLED_DEFAULT_OLLAMA_MODEL`].
pub fn bundled_default_ollama_tag(paths: &AthenaPaths) -> String {
    let marker = paths
        .bundled_ollama_models_dir()
        .join("ATHENA_DEFAULT_MODEL.txt");
    if let Ok(s) = fs::read_to_string(&marker) {
        if let Some(line) = s.lines().find(|l| {
            let t = l.trim();
            !t.is_empty() && !t.starts_with('#')
        }) {
            return line.trim().to_string();
        }
    }
    BUNDLED_DEFAULT_OLLAMA_MODEL.to_string()
}

/// Dual-read helper: prefer `user/<rel>`, else legacy `root/<rel>`.
pub fn resolve_user_or_legacy(root: &Path, rel: &str) -> PathBuf {
    let paths = AthenaPaths::new(root);
    let modern = paths.user_dir().join(rel);
    if modern.exists() {
        return modern;
    }
    let legacy = root.join(rel);
    if legacy.exists() {
        return legacy;
    }
    modern
}

/// Recursively copy `src` → `dst` (for Aegis export of `app/`).
pub fn copy_tree(src: &Path, dst: &Path) -> Result<(), String> {
    if !src.exists() {
        return Ok(());
    }
    copy_dir_recursive(src, dst)
}

/// Seed `app/models/*` from Tauri resource folders when the destination is empty.
/// Call once after [`ensure_layout`] with resolved Resource paths (best-effort).
pub fn seed_app_from_bundled_resources(
    paths: &AthenaPaths,
    ollama_resource: Option<&Path>,
    embedding_resource: Option<&Path>,
    reranker_resource: Option<&Path>,
) -> Result<(), String> {
    ensure_app_skeleton(paths)?;

    if let Some(src) = ollama_resource {
        if ollama_store_has_blobs(src) && !ollama_store_has_blobs(&paths.bundled_ollama_models_dir()) {
            merge_dir_contents(src, &paths.bundled_ollama_models_dir())?;
        }
    }

    if let Some(src) = embedding_resource {
        if src.is_dir() && dir_has_onnx(src) {
            let dst = paths.app_embeddings_dir().join("all-MiniLM-L6-v2");
            if !dir_has_onnx(&dst) {
                merge_dir_contents(src, &dst)?;
            }
            // Also seed writable user store if empty (offline first run).
            let user_dst = paths.user_embeddings_dir().join("all-MiniLM-L6-v2");
            if !dir_has_onnx(&user_dst) {
                merge_dir_contents(src, &user_dst)?;
            }
        }
    }

    if let Some(src) = reranker_resource {
        if src.is_dir() && dir_has_onnx(src) {
            let dst = paths.app_rerankers_dir().join("ms-marco-MiniLM-L-6-v2");
            if !dir_has_onnx(&dst) {
                merge_dir_contents(src, &dst)?;
            }
            let user_dst = paths.user_rerankers_dir().join("ms-marco-MiniLM-L-6-v2");
            if !dir_has_onnx(&user_dst) {
                merge_dir_contents(src, &user_dst)?;
            }
        }
    }

    Ok(())
}

fn dir_has_onnx(dir: &Path) -> bool {
    let onnx = dir.join("model.onnx");
    onnx.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn migrate_flat_layout_to_user() {
        let tmp = env::temp_dir().join(format!(
            "athena_paths_test_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join("models").join("chat")).unwrap();
        fs::write(tmp.join("models").join("chat").join("a.gguf"), b"x").unwrap();
        fs::write(tmp.join(CONFIG_FILENAME), b"{}").unwrap();
        fs::write(tmp.join(DB_FILENAME), b"sqlite").unwrap();
        fs::create_dir_all(tmp.join("maps")).unwrap();
        fs::write(tmp.join("maps").join("x.pmtiles"), b"m").unwrap();

        let paths = ensure_layout(&tmp).expect("layout");
        assert_eq!(paths.read_layout_version(), LAYOUT_VERSION);
        assert!(paths.user_chat_models_dir().join("a.gguf").exists());
        assert!(paths.config_path().exists());
        assert!(paths.db_path().exists());
        assert!(paths.user_maps_dir().join("x.pmtiles").exists());
        assert!(paths.app_dir().join("README.txt").exists());

        let _ = fs::remove_dir_all(&tmp);
    }
}
