//! What is on this station: every offline archive, map and AI model, how big it is, where it is,
//! and whether it works. Answers "what do I already have, and where did it go?" for the Downloads
//! screen without the user having to know the folder layout.

use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InventoryItem {
    /// File or folder name.
    pub name: String,
    /// Human title ("Wikipedia (English)").
    pub title: String,
    /// `archive`, `map`, `model`.
    pub kind: String,
    pub bytes: u64,
    /// Full path on the station.
    pub location: String,
    /// `ready`, `partial` (download not finished), `unreadable` (the reader refused it),
    /// `duplicate` (the same file is also elsewhere).
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Destination {
    /// What goes there ("Library archives").
    pub label: String,
    pub folder: String,
    pub free_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StationInventory {
    pub destinations: Vec<Destination>,
    pub items: Vec<InventoryItem>,
    /// Documents in the chat index (books, PDFs, notes).
    pub indexed_documents: u64,
    /// True when the library reader (kiwix-serve) is answering.
    pub library_reader_running: bool,
    /// kiwix-serve is running but still opening its archives.
    pub library_reader_starting: bool,
}

fn free_bytes(p: &Path) -> Option<u64> {
    let p = p.canonicalize().ok()?;
    let disks = sysinfo::Disks::new_with_refreshed_list();
    disks
        .list()
        .iter()
        .filter(|d| p.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| d.available_space())
}

fn dir_size(p: &Path) -> u64 {
    walkdir::WalkDir::new(p)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

/// Readable title for a Kiwix file name, used when the library reader is not running.
/// `wikipedia_en_medicine_maxi_2026-04.zim` → "Wikipedia: medicine (2026-04)".
pub fn title_from_zim_name(file_name: &str) -> String {
    let stem = file_name.trim_end_matches(".part").trim_end_matches(".zim");
    let mut parts: Vec<&str> = stem.split('_').collect();
    let date = parts
        .last()
        .filter(|d| d.len() >= 7 && d.chars().take(4).all(|c| c.is_ascii_digit()))
        .map(|d| d.to_string());
    if date.is_some() {
        parts.pop();
    }
    let base = parts.first().copied().unwrap_or(stem);
    let known: &[(&str, &str)] = &[
        ("wikipedia", "Wikipedia"),
        ("wikibooks", "Wikibooks"),
        ("wikivoyage", "Wikivoyage"),
        ("wiktionary", "Wiktionary"),
        ("ifixit", "iFixit repair guides"),
        ("stackoverflow.com", "Stack Overflow"),
        ("gutenberg", "Project Gutenberg"),
        ("appropedia", "Appropedia"),
        ("osewiki", "Open Source Ecology wiki"),
        ("mdwiki", "MDWiki medical encyclopedia"),
        ("theworldfactbook", "The World Factbook"),
        ("energypedia", "Energypedia"),
        ("gardenology.org", "Gardenology"),
        ("scoutwiki.org", "ScoutWiki"),
        ("trueprepper.com", "TruePrepper"),
        ("wikivet.net", "WikiVet"),
    ];
    let mut title = known
        .iter()
        .find(|(k, _)| *k == base)
        .map(|(_, v)| v.to_string())
        .unwrap_or_else(|| {
            let b = base.trim_start_matches("zimgit-").replace(['-', '.'], " ");
            let mut c = b.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => b,
            }
        });
    // Topic word after the language ("medicine" in wikipedia_en_medicine_maxi).
    if let Some(topic) = parts.get(2).filter(|t| !matches!(**t, "all" | "maxi" | "mini" | "nopic")) {
        title = format!("{title}: {topic}");
    }
    match date {
        Some(d) => format!("{title} ({d})"),
        None => title,
    }
}

/// Titles the library reader knows, by archive name (file name without `.zim`).
fn reader_titles() -> HashMap<String, String> {
    let url = format!("{}/catalog/v2/entries?count=1000", crate::kiwix_serve::local_base_url());
    let Ok(resp) = ureq::get(&url).timeout(Duration::from_secs(3)).call() else {
        return HashMap::new();
    };
    let Ok(xml) = resp.into_string() else { return HashMap::new() };
    let mut out = HashMap::new();
    for entry in xml.split("<entry>").skip(1) {
        let get = |tag: &str| {
            let open = format!("<{tag}>");
            let close = format!("</{tag}>");
            let s = entry.find(&open)? + open.len();
            let e = entry[s..].find(&close)? + s;
            Some(entry[s..e].trim().to_string())
        };
        if let (Some(name), Some(title)) = (get("name"), get("title")) {
            out.insert(name, title.replace("&amp;", "&"));
        }
    }
    out
}

fn zim_items(athena_root: &Path) -> Vec<InventoryItem> {
    let roots = crate::kiwix_serve::zim_discovery_roots(athena_root);
    let titles = reader_titles();
    let refused = crate::kiwix_serve::refused_list();
    let mut items: Vec<InventoryItem> = Vec::new();
    let mut seen_names: HashMap<String, usize> = HashMap::new();
    for path in crate::kiwix_serve::collect_zim_paths_from_roots(&roots) {
        let pb = PathBuf::from(&path);
        let name = pb.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        let bytes = std::fs::metadata(&pb).map(|m| m.len()).unwrap_or(0);
        let stem = name.trim_end_matches(".zim");
        let title = titles.get(stem).cloned().unwrap_or_else(|| title_from_zim_name(&name));
        let (status, note) = if refused.contains(&path) {
            ("unreadable", Some("The library reader cannot open this file. If it was downloaded by ATHENA, download it again.".to_string()))
        } else if let Some(&first) = seen_names.get(&name.to_ascii_lowercase()) {
            let other = items[first].location.clone();
            ("duplicate", Some(format!("Same archive as {other}. One copy can be deleted to free space.")))
        } else {
            ("ready", None)
        };
        seen_names.entry(name.to_ascii_lowercase()).or_insert(items.len());
        items.push(InventoryItem {
            name,
            title,
            kind: "archive".into(),
            bytes,
            location: plain_path(Path::new(&path)),
            status: status.into(),
            note,
        });
    }
    // Downloads that have not finished.
    let mut part_dirs: Vec<PathBuf> = roots.clone();
    part_dirs.push(crate::athena_paths::AthenaPaths::new(athena_root).user_downloads_dir().join("packages"));
    part_dirs.sort();
    part_dirs.dedup();
    for dir in part_dirs {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if !name.to_ascii_lowercase().ends_with(".zim.part") {
                continue;
            }
            let bytes = e.metadata().map(|m| m.len()).unwrap_or(0);
            items.push(InventoryItem {
                title: title_from_zim_name(&name),
                name,
                kind: "archive".into(),
                bytes,
                location: plain_path(&e.path()),
                status: "partial".into(),
                note: Some("Download not finished. Add it to the queue again to resume where it stopped.".into()),
            });
        }
    }
    items
}

fn map_items(maps_dir: &Path) -> Vec<InventoryItem> {
    walkdir::WalkDir::new(maps_dir)
        .max_depth(3)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let lower = name.to_ascii_lowercase();
            let partial = lower.ends_with(".part");
            let core = lower.trim_end_matches(".part");
            if !(core.ends_with(".pmtiles") || core.ends_with(".mbtiles")) {
                return None;
            }
            let title = name
                .trim_end_matches(".part")
                .trim_end_matches(".pmtiles")
                .trim_end_matches(".mbtiles")
                .replace(['_', '-'], " ");
            Some(InventoryItem {
                title,
                name,
                kind: "map".into(),
                bytes: e.metadata().map(|m| m.len()).unwrap_or(0),
                location: plain_path(&e.path()),
                status: if partial { "partial" } else { "ready" }.into(),
                note: None,
            })
        })
        .collect()
}

fn model_items(models_dir: &Path) -> Vec<InventoryItem> {
    let mut out = Vec::new();
    // Chat models (Ollama): manifests name them, their layers say how big they are.
    let manifests = models_dir.join("ollama").join("manifests").join("registry.ollama.ai").join("library");
    if let Ok(models) = std::fs::read_dir(&manifests) {
        for m in models.flatten() {
            let Ok(tags) = std::fs::read_dir(m.path()) else { continue };
            for t in tags.flatten() {
                let name = format!("{}:{}", m.file_name().to_string_lossy(), t.file_name().to_string_lossy());
                let bytes = std::fs::read_to_string(t.path())
                    .ok()
                    .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                    .and_then(|v| v.get("layers").and_then(|l| l.as_array()).cloned())
                    .map(|layers| layers.iter().filter_map(|l| l.get("size").and_then(|s| s.as_u64())).sum())
                    .unwrap_or(0);
                out.push(InventoryItem {
                    title: format!("Chat model {name}"),
                    name,
                    kind: "model".into(),
                    bytes,
                    location: plain_path(&t.path()),
                    status: "ready".into(),
                    note: None,
                });
            }
        }
    }
    for (sub, label) in [("embeddings", "Search model"), ("rerankers", "Relevance model")] {
        let Ok(rd) = std::fs::read_dir(models_dir.join(sub)) else { continue };
        for d in rd.flatten().filter(|d| d.path().is_dir()) {
            let name = d.file_name().to_string_lossy().into_owned();
            out.push(InventoryItem {
                title: format!("{label} {name}"),
                name,
                kind: "model".into(),
                bytes: dir_size(&d.path()),
                location: plain_path(&d.path()),
                status: if d.path().join("model.onnx").is_file() { "ready" } else { "partial" }.into(),
                note: None,
            });
        }
    }
    out
}

pub fn build(athena_root: &Path, indexed_documents: u64) -> StationInventory {
    let paths = crate::athena_paths::AthenaPaths::new(athena_root);
    let downloads = paths.user_downloads_dir();
    let dest = |label: &str, folder: PathBuf| Destination {
        label: label.to_string(),
        free_bytes: free_bytes(&folder).or_else(|| folder.parent().and_then(free_bytes)),
        folder: plain_path(&folder),
    };
    let destinations = vec![
        dest("Library archives (Wikipedia, manuals, books)", downloads.join("packages")),
        dest("Documents and media", downloads.join("documents")),
        dest("Maps", paths.user_maps_dir()),
        dest("AI models", paths.user_models_dir()),
        dest("Academy lessons", paths.user_library_dir().join("academy")),
    ];
    let mut items = zim_items(athena_root);
    items.extend(map_items(&paths.user_maps_dir()));
    items.extend(model_items(&paths.user_models_dir()));
    StationInventory {
        destinations,
        items,
        indexed_documents,
        library_reader_running: crate::kiwix_serve::kiwix_http_reachable(),
        library_reader_starting: crate::kiwix_serve::still_starting(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_names_read_like_titles() {
        assert_eq!(title_from_zim_name("wikipedia_en_all_maxi_2026-08.zim"), "Wikipedia (2026-08)");
        assert_eq!(title_from_zim_name("wikipedia_en_medicine_maxi_2026-04.zim"), "Wikipedia: medicine (2026-04)");
        assert_eq!(title_from_zim_name("gutenberg_en_all_2025-11.zim.part"), "Project Gutenberg (2025-11)");
        assert_eq!(title_from_zim_name("zimgit-water_en_2024-08.zim"), "Water (2024-08)");
    }

    #[test]
    fn maps_and_partial_maps_are_listed() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("home_area.pmtiles"), b"x").unwrap();
        std::fs::write(dir.path().join("state.pmtiles.part"), b"xy").unwrap();
        std::fs::write(dir.path().join("notes.txt"), b"x").unwrap();
        let mut items = map_items(dir.path());
        items.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "home area");
        assert_eq!(items[0].status, "ready");
        assert_eq!(items[1].status, "partial");
    }
}

/// A path as people should read it: without the Windows `\\?\` long-path prefix.
fn plain_path(p: &Path) -> String {
    let s = p.display().to_string();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        s
    }
}
