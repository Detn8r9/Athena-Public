//! Find useful documents already on this computer and bring them into the library.
//!
//! The scan pulls widely: anything that reads like a manual, guide, map, plan or reference on a
//! survival topic is a candidate, and candidates on topics the library is thin on are marked.
//! Two rules hold no matter what:
//!
//! - Secrets are never pulled (password stores, keys, wallets, bank and tax papers). They are no
//!   use to a library and a hazard if the station is lost.
//! - Personal and location documents (a supply-cache list, a property map) are kept on this
//!   station only. They go to a private shelf that is never offered to other stations.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::coverage::{self, Topic};

pub const ITEM_KIND: &str = "vault_scan_item";
/// Shelf for scan finds that may be shared with other stations.
pub const FOUND_SHELF: &str = "Found on this device";
/// Shelf that is never listed, searched or served to other stations.
pub const PRIVATE_SHELF: &str = "Private (this station only)";

const MIN_USEFULNESS: u32 = 12;
/// Larger finds are listed but wait for an explicit choice before being copied.
const AUTO_IMPORT_MAX_BYTES: u64 = 200 * 1024 * 1024;
const SAMPLE_BYTES: usize = 16 * 1024;
/// A note about a supply cache can be a single line, so only near-empty files are skipped.
const MIN_FILE_BYTES: u64 = 64;
const PDF_SAMPLE_MAX_BYTES: u64 = 25 * 1024 * 1024;

const SKIP_DIRS: &[&str] = &[
    "applications", "node_modules", "target", "appdata", "program files", "program files (x86)", "windows",
    "system", "system volume information", "$recycle.bin", "caches", "cache", "tmp", "temp", "venv", "site-packages",
    "music", "movies", "pictures", "photos",
];

/// Lower-case fragments of a path that mark a file as a secret. Checked against the whole path.
const NEVER_PULL: &[&str] = &[
    "password", "passwd", "passcode", "keychain", "1password", "lastpass", "bitwarden", "keepass", "wallet", "seed phrase",
    "seed-phrase", "recovery phrase", "recovery key", "recovery-key", "private key", "private-key", "id_rsa", "id_ed25519", "/.ssh/",
    "/.gnupg/", "credential", "secret", "api key", "api-key", "apikey", "token", "tax return", "tax-return", "taxes",
    "bank statement", "bank-statement", "social security", "passport", "credit card", "credit-card", "payslip", "pay stub",
    "paystub", "w-2", "w2 ", "1099", "invoice", "receipt", "resume", "curriculum vitae", "2fa", "backup codes", "backup-codes",
];
const NEVER_PULL_EXT: &[&str] = &["kdbx", "kdb", "key", "pem", "p12", "pfx", "keystore", "jks", "ovpn", "env", "asc", "gpg"];

/// Words that mark a useful document as personal: kept, but only on this station.
const PRIVATE_WORDS: &[&str] = &[
    "confidential", "private", "personal", "medical record", "medical history", "prescription", "diagnosis", "insurance",
    "birth certificate", "marriage", "will and testament", "deed", "lease", "mortgage", "home address", "phone numbers",
    "contact list", "family", "next of kin", "emergency contacts",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemStatus {
    Found,
    Imported,
    Dismissed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanItem {
    pub path: String,
    pub name: String,
    pub bytes: u64,
    pub modified_at: i64,
    pub topic: Option<String>,
    pub topic_label: Option<String>,
    /// 0-100: how much this looks like useful reference material.
    pub score: u32,
    /// Kept on this station only.
    pub private: bool,
    /// On a topic the library has little or nothing about.
    pub fills_blind_spot: bool,
    /// Too large to copy without asking.
    pub large: bool,
    pub status: ItemStatus,
    #[serde(default)]
    pub imported_to: Option<String>,
}

/// Folders worth scanning on this computer when the user names none.
pub fn default_roots() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from);
    let Some(home) = home else { return Vec::new() };
    ["Documents", "Downloads", "Desktop"].iter().map(|d| home.join(d)).filter(|p| p.is_dir()).collect()
}

pub fn is_never_pull(path: &Path) -> bool {
    let lower = path.to_string_lossy().to_lowercase().replace('\\', "/");
    if NEVER_PULL.iter().any(|w| lower.contains(w)) {
        return true;
    }
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| NEVER_PULL_EXT.contains(&e.to_ascii_lowercase().as_str()))
}

pub fn looks_private(name: &str, sample: &str, topic: Option<Topic>) -> bool {
    if topic == Some(Topic::Local) {
        return true;
    }
    let text = format!("{} {}", name.to_lowercase(), sample.to_lowercase());
    PRIVATE_WORDS.iter().any(|w| text.contains(w))
}

fn skip_dir(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.starts_with('.')
        || SKIP_DIRS.contains(&lower.as_str())
        || lower.ends_with(".app")
        || lower.ends_with(".photoslibrary")
        || lower.ends_with(".bundle")
}

fn is_candidate_type(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase().as_str(),
        "pdf" | "txt" | "md" | "html" | "htm" | "docx" | "doc" | "epub" | "csv" | "zim"
    )
}

/// A little of the document's text, enough to tell what it is about. Never the whole file.
fn sample_text(path: &Path, bytes: u64) -> String {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "txt" | "md" | "csv" | "html" | "htm" => {
            use std::io::Read;
            let mut buf = vec![0u8; SAMPLE_BYTES.min(bytes as usize)];
            let Ok(mut f) = std::fs::File::open(path) else { return String::new() };
            let n = f.read(&mut buf).unwrap_or(0);
            String::from_utf8_lossy(&buf[..n]).into_owned()
        }
        "pdf" if bytes <= PDF_SAMPLE_MAX_BYTES => {
            let path = path.to_path_buf();
            std::panic::catch_unwind(move || {
                let doc = lopdf::Document::load(&path).ok()?;
                let pages: Vec<u32> = doc.get_pages().keys().copied().take(3).collect();
                doc.extract_text(&pages).ok()
            })
            .ok()
            .flatten()
            .map(|t| t.chars().take(SAMPLE_BYTES).collect())
            .unwrap_or_default()
        }
        _ => String::new(),
    }
}

/// Judge one file. `None` means it is not worth keeping (or must not be touched).
pub fn judge(path: &Path, bytes: u64, modified_at: i64, sample: &str, blind_spots: &[Topic]) -> Option<ScanItem> {
    if is_never_pull(path) {
        return None;
    }
    let name = path.file_name()?.to_string_lossy().into_owned();
    let score = coverage::usefulness(&name, sample);
    if score < MIN_USEFULNESS {
        return None;
    }
    let topic = coverage::best_topic(&name, sample);
    Some(ScanItem {
        path: path.to_string_lossy().into_owned(),
        name: name.clone(),
        bytes,
        modified_at,
        topic: topic.map(|t| t.key().to_string()),
        topic_label: topic.map(|t| t.label().to_string()),
        score,
        private: looks_private(&name, sample, topic),
        fills_blind_spot: topic.is_some_and(|t| blind_spots.contains(&t)),
        large: bytes > AUTO_IMPORT_MAX_BYTES,
        status: ItemStatus::Found,
        imported_to: None,
    })
}

pub struct ScanOutcome {
    pub items: Vec<ScanItem>,
    pub files_seen: u64,
}

/// Walk `roots` and return the useful documents, best first. `exclude` are folders to leave out
/// (ATHENA's own data). `already` holds (file name, size) pairs the library has; those are
/// skipped. `on_progress(files seen)` is called now and then; `should_stop` ends the walk early.
pub fn scan(
    roots: &[PathBuf],
    exclude: &[PathBuf],
    already: &HashSet<(String, u64)>,
    blind_spots: &[Topic],
    mut on_progress: impl FnMut(u64),
    should_stop: impl Fn() -> bool,
) -> ScanOutcome {
    let mut items = Vec::new();
    let mut files_seen = 0u64;
    let mut seen_paths = HashSet::new();
    for root in roots {
        let walker = walkdir::WalkDir::new(root).follow_links(false).max_depth(12).into_iter().filter_entry(|e| {
            if e.depth() == 0 {
                return true;
            }
            let name = e.file_name().to_string_lossy();
            if e.file_type().is_dir() {
                // The operating system's own "Library" sits at the top of a home folder; a
                // "Library" folder of books deeper down is exactly what the scan is for.
                let os_library = e.depth() == 1 && name.eq_ignore_ascii_case("library");
                return !os_library && !skip_dir(&name) && !exclude.iter().any(|x| e.path().starts_with(x));
            }
            !name.starts_with('.')
        });
        for entry in walker.filter_map(|e| e.ok()) {
            if should_stop() {
                break;
            }
            let path = entry.path();
            if !entry.file_type().is_file() || !is_candidate_type(path) {
                continue;
            }
            files_seen += 1;
            if files_seen % 200 == 0 {
                on_progress(files_seen);
            }
            let Ok(meta) = entry.metadata() else { continue };
            let bytes = meta.len();
            if bytes < MIN_FILE_BYTES || !seen_paths.insert(path.to_path_buf()) {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if already.contains(&(name, bytes)) || is_never_pull(path) {
                continue;
            }
            let modified_at = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            // Only read into a file whose name alone did not already settle it.
            let by_name = judge(path, bytes, modified_at, "", blind_spots);
            let item = match by_name {
                Some(i) if i.score >= 40 => Some(i),
                _ => judge(path, bytes, modified_at, &sample_text(path, bytes), blind_spots),
            };
            if let Some(item) = item {
                items.push(item);
            }
        }
    }
    items.sort_by(|a, b| {
        b.fills_blind_spot
            .cmp(&a.fills_blind_spot)
            .then(b.score.cmp(&a.score))
            .then(a.name.cmp(&b.name))
    });
    ScanOutcome { items, files_seen }
}

/// Where an item is copied to inside the library.
pub fn import_destination(library_dir: &Path, item: &ScanItem) -> PathBuf {
    let shelf = if item.private { PRIVATE_SHELF } else { FOUND_SHELF };
    let topic = item.topic_label.as_deref().unwrap_or("Other");
    library_dir.join(shelf).join(topic).join(&item.name)
}

/// Copy an item into the library. The source is left where it was. A different file already at
/// the destination is kept; the new copy gets a numbered name.
pub fn import_item(library_dir: &Path, item: &ScanItem) -> Result<PathBuf, String> {
    let src = Path::new(&item.path);
    if is_never_pull(src) {
        return Err("This file is on the never-copy list.".to_string());
    }
    let mut dest = import_destination(library_dir, item);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut n = 1;
    while dest.exists() {
        if dest.metadata().map(|m| m.len()).unwrap_or(0) == item.bytes {
            return Ok(dest);
        }
        n += 1;
        let stem = src.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let ext = src.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
        dest = dest.with_file_name(format!("{stem} ({n}){ext}"));
    }
    let part = dest.with_extension("athena-part");
    std::fs::copy(src, &part).map_err(|e| format!("Could not copy {}: {e}", item.name))?;
    std::fs::rename(&part, &dest).map_err(|e| e.to_string())?;
    Ok(dest)
}

/// True for any path on the private shelf. Used wherever files are offered to other stations.
pub fn is_private_path(path: &str) -> bool {
    path.replace('\\', "/").contains(&format!("/{PRIVATE_SHELF}/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, rel: &str, body: &str) -> PathBuf {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, body).unwrap();
        p
    }

    #[test]
    fn secrets_are_never_candidates() {
        for p in [
            "/Users/x/Documents/passwords.txt",
            "/Users/x/Documents/2025 tax return.pdf",
            "/Users/x/.ssh/id_rsa",
            "/Users/x/Documents/vault.kdbx",
            "/Users/x/Documents/Bank Statement March.pdf",
            "/Users/x/Documents/wallet seed phrase.txt",
        ] {
            assert!(is_never_pull(Path::new(p)), "{p}");
            assert!(judge(Path::new(p), 5000, 0, "solar battery water purification manual", &[]).is_none(), "{p}");
        }
        assert!(!is_never_pull(Path::new("/Users/x/Documents/Diesel engine repair manual.pdf")));
    }

    #[test]
    fn a_cache_list_is_kept_private_and_a_manual_is_shareable() {
        let cache = judge(Path::new("/d/notes.txt"), 2000, 0, "Supply cache is hidden behind the north well. Coordinates: 44.1, -103.2", &[]).unwrap();
        assert!(cache.private);
        assert_eq!(cache.topic.as_deref(), Some("local"));
        let manual = judge(Path::new("/d/Diesel engine repair manual.pdf"), 2000, 0, "", &[Topic::Vehicles]).unwrap();
        assert!(!manual.private);
        assert!(manual.fills_blind_spot);
        assert!(judge(Path::new("/d/shopping.txt"), 2000, 0, "milk eggs bread", &[]).is_none());
    }

    #[test]
    fn scan_finds_useful_files_and_skips_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        let body = "x".repeat(600);
        write(dir.path(), "Documents/Diesel engine repair manual.txt", &body);
        write(dir.path(), "Documents/cache.txt", &format!("The supply cache is hidden near the old well. Coordinates follow. {body}"));
        write(dir.path(), "Documents/passwords.txt", &format!("solar manual {body}"));
        write(dir.path(), "Documents/node_modules/solar manual.txt", &body);
        write(dir.path(), "Documents/.hidden/water purification guide.txt", &body);
        write(dir.path(), "Documents/holiday.txt", &body);
        write(dir.path(), "Documents/First aid handbook.txt", &body);
        write(dir.path(), "Documents/notes/north.txt", "Supply cache hidden behind the north well. Rally point at the barn.");
        write(dir.path(), "AthenaData/Water purification guide.txt", &body);

        let mut already = HashSet::new();
        already.insert(("First aid handbook.txt".to_string(), 600u64));
        let out = scan(
            &[dir.path().to_path_buf()],
            &[dir.path().join("AthenaData")],
            &already,
            &[Topic::Vehicles],
            |_| {},
            || false,
        );
        let names: Vec<&str> = out.items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, vec!["Diesel engine repair manual.txt", "cache.txt", "north.txt"], "blind-spot fill comes first");
        assert!(out.items[1].private && out.items[2].private, "a one-line cache note is found and kept private");
    }

    #[test]
    fn import_copies_to_the_right_shelf_and_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let src = write(dir.path(), "src/Diesel engine repair manual.txt", &"manual ".repeat(200));
        let lib = dir.path().join("library");
        let item = judge(&src, 1400, 0, "", &[]).unwrap();
        let dest = import_item(&lib, &item).unwrap();
        assert_eq!(dest, lib.join(FOUND_SHELF).join("Vehicles and engines").join("Diesel engine repair manual.txt"));
        assert!(src.exists(), "the source stays where it was");
        // Same size again: the existing copy is reused.
        assert_eq!(import_item(&lib, &item).unwrap(), dest);
        // A different file with the same name is kept; the new one gets a number.
        std::fs::write(&dest, "different").unwrap();
        let second = import_item(&lib, &item).unwrap();
        assert!(second.to_string_lossy().ends_with("Diesel engine repair manual (2).txt"));

        let mut private = item.clone();
        private.private = true;
        assert!(is_private_path(&import_item(&lib, &private).unwrap().to_string_lossy()));
        assert!(!is_private_path(&dest.to_string_lossy()));
    }
}
