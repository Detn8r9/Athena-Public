//! Newer releases of installed offline encyclopedias.
//!
//! Kiwix names every release `<base>_<YYYY-MM>.zim` and lists them in plain folder pages. ATHENA
//! reads the folder page, finds a later date for a base it has, downloads it, checks that it
//! opens, and only then removes the older file.

use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const KIWIX_ZIM_ROOT: &str = "https://download.kiwix.org/zim";

/// Split `wikipedia_en_medicine_nopic_2026-10.zim` into its base and release date.
pub fn split_release(file_name: &str) -> Option<(String, String)> {
    let stem = file_name.strip_suffix(".zim")?;
    let (base, date) = stem.rsplit_once('_')?;
    let b = date.as_bytes();
    let dated = b.len() >= 7
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[4] == b'-'
        && b[5..7].iter().all(u8::is_ascii_digit)
        && b[7..].iter().all(|c| c.is_ascii_lowercase());
    (dated && !base.is_empty()).then(|| (base.to_string(), date.to_string()))
}

/// The Kiwix folder a base name is published in.
pub fn folder_for(base: &str) -> &'static str {
    const PROJECTS: &[&str] = &[
        "wikipedia", "wikibooks", "wiktionary", "wikivoyage", "wikiversity", "wikiquote", "wikisource", "wikinews",
        "vikidia", "phet", "ifixit", "gutenberg", "ted", "mooc", "videos",
    ];
    if base.contains(".stackexchange.com") || base.starts_with("stackoverflow.com") {
        return "stack_exchange";
    }
    if base.starts_with("libretexts.org") {
        return "libretexts";
    }
    if let Some(p) = PROJECTS.iter().find(|p| base.starts_with(&format!("{p}_"))) {
        return p;
    }
    // Sites captured with the Zimit crawler are named after their domain.
    let first = base.split('_').next().unwrap_or("");
    if first.contains('.') {
        return "zimit";
    }
    "other"
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListedFile {
    pub name: String,
    /// Size as the listing prints it, such as `326M` or `4.0G`.
    pub size_text: String,
}

/// File names and sizes from a Kiwix folder page (an Apache-style index).
pub fn parse_listing(html: &str) -> Vec<ListedFile> {
    let mut out = Vec::new();
    for line in html.lines() {
        let Some(i) = line.find("href=\"") else { continue };
        let rest = &line[i + 6..];
        let Some(q) = rest.find('"') else { continue };
        let name = &rest[..q];
        if !name.ends_with(".zim") || name.contains('/') {
            continue;
        }
        let size_text = line
            .rsplit("</a>")
            .next()
            .and_then(|tail| tail.split_whitespace().last())
            .filter(|s| s.chars().next().is_some_and(|c| c.is_ascii_digit()))
            .unwrap_or("")
            .to_string();
        out.push(ListedFile { name: name.to_string(), size_text });
    }
    out
}

/// The latest release of `base` in a listing.
pub fn newest<'a>(listing: &'a [ListedFile], base: &str) -> Option<&'a ListedFile> {
    listing
        .iter()
        .filter_map(|f| split_release(&f.name).filter(|(b, _)| b == base).map(|(_, date)| (date, f)))
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, f)| f)
}

/// `326M` -> bytes. Unknown text gives `None`.
pub fn size_text_bytes(text: &str) -> Option<u64> {
    let t = text.trim();
    let (num, unit) = t.split_at(t.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(t.len()));
    let n: f64 = num.parse().ok()?;
    let mult = match unit.to_ascii_uppercase().as_str() {
        "" | "B" => 1.0,
        "K" => 1024.0,
        "M" => 1024.0 * 1024.0,
        "G" => 1024.0 * 1024.0 * 1024.0,
        "T" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some((n * mult) as u64)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub installed_file: String,
    pub installed_path: String,
    pub installed_date: String,
    pub latest_file: String,
    pub latest_date: String,
    pub url: String,
    pub size_text: String,
    pub size_bytes: Option<u64>,
}

/// Updates for the installed files, given the listing of each folder.
pub fn find_updates(installed: &[PathBuf], listings: &HashMap<&'static str, Vec<ListedFile>>) -> Vec<UpdateInfo> {
    let mut out = Vec::new();
    // Several dated copies of one base may be installed; only the newest of them matters.
    let mut newest_installed: HashMap<String, (String, &PathBuf)> = HashMap::new();
    for path in installed {
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
        let Some((base, date)) = split_release(name) else { continue };
        let slot = newest_installed.entry(base).or_insert_with(|| (date.clone(), path));
        if date > slot.0 {
            *slot = (date, path);
        }
    }
    for (base, (date, path)) in newest_installed {
        let folder = folder_for(&base);
        let Some(latest) = listings.get(folder).and_then(|l| newest(l, &base)) else { continue };
        let Some((_, latest_date)) = split_release(&latest.name) else { continue };
        if latest_date <= date {
            continue;
        }
        out.push(UpdateInfo {
            installed_file: path.file_name().unwrap_or_default().to_string_lossy().into_owned(),
            installed_path: path.to_string_lossy().into_owned(),
            installed_date: date,
            latest_file: latest.name.clone(),
            latest_date,
            url: format!("{KIWIX_ZIM_ROOT}/{folder}/{}", latest.name),
            size_text: latest.size_text.clone(),
            size_bytes: size_text_bytes(&latest.size_text),
        });
    }
    out.sort_by(|a, b| a.installed_file.cmp(&b.installed_file));
    out
}

/// Fetch one Kiwix folder page. Sends nothing about this station beyond a generic client name.
pub fn fetch_listing(folder: &str) -> Result<Vec<ListedFile>, String> {
    let url = format!("{KIWIX_ZIM_ROOT}/{folder}/");
    let client = reqwest::blocking::Client::builder()
        .user_agent("ATHENA-station")
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let body = client
        .get(&url)
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.text())
        .map_err(|e| format!("Could not read the Kiwix listing for {folder}: {e}"))?;
    Ok(parse_listing(&body))
}

/// Check every installed archive against the live listings.
pub fn check_updates(installed: &[PathBuf]) -> Result<Vec<UpdateInfo>, String> {
    let mut folders: Vec<&'static str> = installed
        .iter()
        .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
        .filter_map(split_release)
        .map(|(base, _)| folder_for(&base))
        .collect();
    folders.sort_unstable();
    folders.dedup();
    let mut listings = HashMap::new();
    let mut last_err = None;
    for folder in folders {
        match fetch_listing(folder) {
            Ok(l) => {
                listings.insert(folder, l);
            }
            Err(e) => last_err = Some(e),
        }
    }
    if listings.is_empty() {
        if let Some(e) = last_err {
            return Err(e);
        }
    }
    Ok(find_updates(installed, &listings))
}

/// The current download for a suggested source: (url, file name, size text).
pub fn resolve_latest(folder: &str, base: &str) -> Result<ListedFile, String> {
    let listing = fetch_listing(folder)?;
    newest(&listing, base).cloned().ok_or_else(|| format!("{base} is not in the current Kiwix listing."))
}

/// After an update: remove the older file, but only once the newer one is in place and opens.
pub fn remove_superseded(old: &Path, new: &Path) -> Result<(), String> {
    let (Some(old_name), Some(new_name)) = (
        old.file_name().and_then(|n| n.to_str()).and_then(split_release),
        new.file_name().and_then(|n| n.to_str()).and_then(split_release),
    ) else {
        return Err("Both files must be dated Kiwix releases.".to_string());
    };
    if old_name.0 != new_name.0 || new_name.1 <= old_name.1 {
        return Err("The new file is not a later release of the same archive.".to_string());
    }
    if !new.is_file() {
        return Err("The new release is not installed yet.".to_string());
    }
    crate::indexer::open_zim(new).map_err(|e| format!("The new release does not open, so the old one was kept: {e}"))?;
    if old.is_file() {
        std::fs::remove_file(old).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const LISTING: &str = r#"<pre><a href="../">../</a>
      <a href="mechanics.stackexchange.com_en_all_2026-08.zim">mechanics.stackexchange.com_en_all_2026-08.zim</a>           2026-08-21 16:34  324M
      <a href="mechanics.stackexchange.com_en_all_2026-10.zim">mechanics.stackexchange.com_en_all_2026-10.zim</a>           2026-10-20 06:01  326M
      <a href="ham.stackexchange.com_en_all_2026-10.zim">ham.stackexchange.com_en_all_2026-10.zim</a>                 2026-10-20 11:06   74M
      <a href="readme.txt">readme.txt</a> 2020-01-01 00:00 1K
    </pre>"#;

    #[test]
    fn release_names_split_into_base_and_date() {
        assert_eq!(
            split_release("wikipedia_en_medicine_nopic_2026-10.zim"),
            Some(("wikipedia_en_medicine_nopic".to_string(), "2026-10".to_string()))
        );
        assert_eq!(split_release("vikidia_en_all_nopic_2026-09a.zim").unwrap().1, "2026-09a");
        assert_eq!(split_release("athena-pack_medical_2026-11-12.zim"), None);
        assert_eq!(split_release("notes.zim"), None);
    }

    #[test]
    fn bases_map_to_their_kiwix_folder() {
        assert_eq!(folder_for("mechanics.stackexchange.com_en_all"), "stack_exchange");
        assert_eq!(folder_for("wikipedia_en_medicine_nopic"), "wikipedia");
        assert_eq!(folder_for("libretexts.org_en_k12"), "libretexts");
        assert_eq!(folder_for("irp.fas.org_en_military-medicine"), "zimit");
        assert_eq!(folder_for("appropedia_en_all_maxi"), "other");
        assert_eq!(folder_for("zimgit-water_en"), "other");
    }

    #[test]
    fn listing_gives_names_sizes_and_the_newest_release() {
        let files = parse_listing(LISTING);
        assert_eq!(files.len(), 3);
        let newest = newest(&files, "mechanics.stackexchange.com_en_all").unwrap();
        assert_eq!(newest.name, "mechanics.stackexchange.com_en_all_2026-10.zim");
        assert_eq!(newest.size_text, "326M");
        assert_eq!(size_text_bytes("326M"), Some(326 * 1024 * 1024));
        assert_eq!(size_text_bytes("4.0G"), Some(4 * 1024 * 1024 * 1024));
        assert_eq!(size_text_bytes("?"), None);
    }

    #[test]
    fn only_older_installs_get_an_update() {
        let mut listings = HashMap::new();
        listings.insert("stack_exchange", parse_listing(LISTING));
        let installed = vec![
            PathBuf::from("/z/mechanics.stackexchange.com_en_all_2026-08.zim"),
            PathBuf::from("/z/ham.stackexchange.com_en_all_2026-10.zim"),
            PathBuf::from("/z/athena-pack_medical_2026-11-12.zim"),
        ];
        let updates = find_updates(&installed, &listings);
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].latest_date, "2026-10");
        assert_eq!(updates[0].url, "https://download.kiwix.org/zim/stack_exchange/mechanics.stackexchange.com_en_all_2026-10.zim");
    }

    #[test]
    fn the_old_release_is_kept_unless_the_new_one_is_good() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("ham.stackexchange.com_en_all_2026-08.zim");
        let new = dir.path().join("ham.stackexchange.com_en_all_2026-10.zim");
        std::fs::write(&old, b"old").unwrap();
        assert!(remove_superseded(&old, &new).unwrap_err().contains("not installed"));
        std::fs::write(&new, b"not a zim").unwrap();
        assert!(remove_superseded(&old, &new).unwrap_err().contains("does not open"));
        assert!(old.exists());
        assert!(remove_superseded(&new, &old).is_err(), "an older file never replaces a newer one");
    }
}
