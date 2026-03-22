// Kiwix ZIM catalog: uses library.kiwix.org OPDS API for server-side search, filter, and
// pagination (full catalog experience). Falls back to cached full catalog when API is
// unavailable.

use std::io::{BufReader, Write};
use std::path::Path;
use serde::Serialize;

const KIWIX_BASE: &str = "https://download.kiwix.org/zim";
/// library.kiwix.org OPDS catalog — server-side search/filter/pagination.
const OPDS_ENTRIES: &str = "https://library.kiwix.org/catalog/v2/entries";
const OPDS_CATEGORIES: &str = "https://library.kiwix.org/catalog/v2/categories";
const OPDS_LANGUAGES: &str = "https://library.kiwix.org/catalog/v2/languages";
const OPDS_PAGE_SIZE: usize = 10;
/// Standard User-Agent so Kiwix and mirrors don't reject the request as a generic bot.
const OPDS_USER_AGENT: &str = "Mozilla/5.0 (compatible; Athena/1.0; Kiwix catalog client)";
const LIBRARY_URL: &str = "https://download.kiwix.org/library/library_zim.xml";
const LIBRARY_MIRROR: &str = "https://ftp.nluug.nl/pub/sites/download.kiwix.org/library/library_zim.xml";
const OPDS_TIMEOUT_SECS: u64 = 60;
/// Timeout for the full library XML (~19MB). At least 120s; use 600s so slow links can finish.
const LIBRARY_XML_TIMEOUT_SECS: u64 = 600;
const ACCEPT_XML: &str = "application/xml, text/xml, */*";

fn format_size(bytes: u64) -> String {
    const GB: u64 = 1_000_000_000;
    const MB: u64 = 1_000_000;
    const KB: u64 = 1_000;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct KiwixEntry {
    pub id: String,
    pub title: String,
    pub description: String,
    pub url: String,
    pub size_bytes: u64,
    pub size_label: String,
    /// Category from API (e.g. wikipedia, stack_exchange, ted).
    #[serde(default)]
    pub category: String,
    /// Language code(s) from API (e.g. eng, fra).
    #[serde(default)]
    pub language: String,
    /// Tags string from API for filtering.
    #[serde(default)]
    pub tags: String,
}

/// Curated fallback when the full library cannot be fetched.
pub fn get_catalog_fallback() -> Vec<KiwixEntry> {
    vec![
        KiwixEntry {
            id: "ifixit_en".to_string(),
            title: "iFixit Repair Guides (English)".to_string(),
            description: "Step-by-step repair guides for devices, appliances, and more. 44,000+ guides.".to_string(),
            url: format!("{}/ifixit/ifixit_en_all_2025-12.zim", KIWIX_BASE),
            size_bytes: 3_300_000_000,
            size_label: "3.3 GB".to_string(),
            category: "ifixit".to_string(),
            language: "eng".to_string(),
            tags: "ifixit".to_string(),
        },
        KiwixEntry {
            id: "wikimed_en_mini".to_string(),
            title: "Wikipedia Medicine (English, compact)".to_string(),
            description: "Medical and health articles from Wikipedia. Good for first aid and reference.".to_string(),
            url: format!("{}/wikipedia/wikipedia_en_medicine_mini_2026-01.zim", KIWIX_BASE),
            size_bytes: 155_000_000,
            size_label: "155 MB".to_string(),
            category: "wikipedia".to_string(),
            language: "eng".to_string(),
            tags: "wikipedia;medicine".to_string(),
        },
        KiwixEntry {
            id: "wikimed_en_nopic".to_string(),
            title: "Wikipedia Medicine (English, full text)".to_string(),
            description: "Larger medical Wikipedia archive without images. More articles than mini.".to_string(),
            url: format!("{}/wikipedia/wikipedia_en_medicine_nopic_2026-01.zim", KIWIX_BASE),
            size_bytes: 820_000_000,
            size_label: "820 MB".to_string(),
            category: "wikipedia".to_string(),
            language: "eng".to_string(),
            tags: "wikipedia;medicine".to_string(),
        },
    ]
}

/// Returns the full catalog (from library.kiwix.org OPDS when possible).
pub fn get_catalog() -> Vec<KiwixEntry> {
    get_catalog_cached(None, false)
}

/// Sort order for catalog page.
#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KiwixSort {
    TitleAsc,
    TitleDesc,
    SizeAsc,
    SizeDesc,
}

/// Facets for filter dropdowns (returned when offset=0).
#[derive(Debug, Clone, Serialize)]
pub struct KiwixFacets {
    pub categories: Vec<String>,
    pub languages: Vec<String>,
}

/// Response for a single page of catalog entries (server-side filter/sort).
#[derive(Debug, Clone, Serialize)]
pub struct KiwixCatalogPage {
    pub entries: Vec<KiwixEntry>,
    pub total: u32,
    pub has_more: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facets: Option<KiwixFacets>,
    /// User-facing message when catalog is limited (e.g. fallback, connection issue).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Returns one page of catalog entries. Always uses the full catalog: fetches and caches it once
/// (library XML or OPDS full fetch), then filters/sorts and returns the requested slice.
/// This avoids freezing the UI when loading the full catalog and lets search/filter run on the entire set.
pub fn get_catalog_page(
    cache_path: Option<&Path>,
    force_refresh: bool,
    search: &str,
    category: &str,
    language: &str,
    sort: KiwixSort,
    offset: u32,
    limit: u32,
) -> KiwixCatalogPage {
    let limit = limit.min(100).max(1);
    let no_filters = search.trim().is_empty() && (category == "all" || category.is_empty()) && (language == "all" || language.is_empty());

    // Full catalog from cache (or fetch once and cache); then filter/sort/slice.
    let catalog = get_catalog_cached(cache_path, force_refresh);
    let catalog_len = catalog.len();
    let facets = if offset == 0 {
        let mut categories: Vec<String> = catalog
            .iter()
            .filter_map(|e| if e.category.is_empty() { None } else { Some(e.category.clone()) })
            .collect();
        categories.sort();
        categories.dedup();
        let mut languages: Vec<String> = catalog
            .iter()
            .flat_map(|e| {
                e.language
                    .split(|c| c == ',' || c == ';')
                    .map(|s| s.trim().to_lowercase())
                    .filter(|s| !s.is_empty())
            })
            .collect();
        languages.sort();
        languages.dedup();
        Some(KiwixFacets {
            categories,
            languages,
        })
    } else {
        None
    };
    let q = search.trim().to_lowercase();
    let mut filtered: Vec<KiwixEntry> = catalog
        .into_iter()
        .filter(|e| {
            if !q.is_empty() {
                let match_search = e.title.to_lowercase().contains(&q)
                    || e.description.to_lowercase().contains(&q)
                    || e.id.to_lowercase().contains(&q)
                    || e.tags.to_lowercase().contains(&q)
                    || e.category.to_lowercase().contains(&q)
                    || e.language.to_lowercase().contains(&q);
                if !match_search {
                    return false;
                }
            }
            if category == "__size_small__" && e.size_bytes >= 500 * 1_000_000 {
                return false;
            }
            if category != "all" && category != "__size_small__" && e.category != category {
                return false;
            }
            if language != "all" {
                let entry_langs: Vec<String> = e
                    .language
                    .split(|c| c == ',' || c == ';')
                    .map(|s| s.trim().to_lowercase())
                    .filter(|s| !s.is_empty())
                    .collect();
                let lang_lower = language.to_lowercase();
                if !entry_langs.iter().any(|l| l == &lang_lower) {
                    return false;
                }
            }
            true
        })
        .collect();
    match sort {
        KiwixSort::TitleAsc => filtered.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase())),
        KiwixSort::TitleDesc => filtered.sort_by(|a, b| b.title.to_lowercase().cmp(&a.title.to_lowercase())),
        KiwixSort::SizeAsc => filtered.sort_by_key(|e| e.size_bytes),
        KiwixSort::SizeDesc => filtered.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes)),
    }
    let total = filtered.len() as u32;
    let offset = offset.min(total);
    let start = offset as usize;
    let end = (offset + limit).min(total) as usize;
    let entries: Vec<KiwixEntry> = if start >= filtered.len() {
        vec![]
    } else {
        filtered[start..end].to_vec()
    };
    // When we only have the 3 hardcoded fallback entries, inform the user.
    let message = if catalog_len == 3 && no_filters && offset == 0 {
        Some("Couldn't load the full Kiwix catalog. Showing 3 sample entries. Check your internet connection and click Refresh to try again.".to_string())
    } else if catalog_len > 3 && offset == 0 && no_filters {
        Some("Loaded from cache. Click Refresh to fetch the latest catalog from Kiwix.".to_string())
    } else {
        None
    };
    KiwixCatalogPage {
        has_more: (end as u32) < total,
        total,
        entries,
        facets,
        message,
    }
}

/// Returns the full catalog. Tries JSON cache, then download.kiwix.org library XML (single large file),
/// then library.kiwix.org OPDS. Caches to cache_dir as JSON when we get a full catalog.
/// No hardcoded limit: pagination is applied by get_catalog_page; only fallback returns 3 sample entries.
pub fn get_catalog_cached(cache_path: Option<&Path>, force_refresh: bool) -> Vec<KiwixEntry> {
    let cache_file = cache_path.map(|p| p.parent().unwrap_or(p).join("library_kiwix_catalog.json"));
    if let Some(ref path) = cache_file {
        let cache_dir = path.parent().unwrap_or_else(|| Path::new("."));
        eprintln!("[Kiwix] Cache file path: {}, directory: {}", path.display(), cache_dir.display());
        if path.exists() && !force_refresh {
            match std::fs::read(path) {
                Ok(data) => match serde_json::from_slice::<Vec<KiwixEntry>>(&data) {
                    Ok(entries) => {
                        if entries.len() >= 100 {
                            eprintln!("[Kiwix] Using cached catalog: {} entries from {}", entries.len(), path.display());
                            return entries;
                        }
                        eprintln!("[Kiwix] Cached catalog has only {} entries (need >= 100), skipping", entries.len());
                    }
                    Err(e) => eprintln!("[Kiwix] Failed to parse cache file {}: {}", path.display(), e),
                },
                Err(e) => eprintln!("[Kiwix] Failed to read cache file {} (permissions/storage?): {}", path.display(), e),
            }
        }
    } else {
        eprintln!("[Kiwix] No cache_path provided; cache file path is unavailable");
    }

    // 1) Try download.kiwix.org library XML first (single ~19MB file, often more reliable than OPDS)
    if let Some(cache) = cache_path {
        let xml_path = cache.parent().unwrap_or(cache).join("library_zim.xml");
        eprintln!("[Kiwix] Library XML path: {}", xml_path.display());
        if force_refresh || !xml_path.exists() {
            match fetch_library_to_file(&xml_path) {
                Ok(()) => eprintln!("[Kiwix] Fetched library XML to {}", xml_path.display()),
                Err(e) => eprintln!("[Kiwix] Failed to fetch library XML: {}", e),
            }
        }
        if xml_path.exists() {
            match parse_library_xml(&xml_path) {
                Ok(entries) => {
                    if entries.len() >= 100 {
                        eprintln!("[Kiwix] Parsed library XML: {} entries", entries.len());
                        if let Some(ref path) = cache_file {
                            if let Some(parent) = path.parent() {
                                match std::fs::create_dir_all(parent) {
                                    Ok(()) => {}
                                    Err(e) => eprintln!("[Kiwix] Failed to create cache directory {} (permissions?): {}", parent.display(), e),
                                }
                                let json = match serde_json::to_string_pretty(&entries) {
                                    Ok(s) => s,
                                    Err(e) => {
                                        eprintln!("[Kiwix] Failed to serialize catalog to JSON: {}", e);
                                        return entries;
                                    }
                                };
                                match std::fs::write(path, json) {
                                    Ok(()) => eprintln!("[Kiwix] Wrote cache to {}", path.display()),
                                    Err(e) => eprintln!("[Kiwix] Failed to write cache file {} (permissions or storage?): {}", path.display(), e),
                                }
                            }
                        }
                        return entries;
                    }
                    eprintln!("[Kiwix] Library XML parsed to only {} entries (need >= 100)", entries.len());
                }
                Err(e) => eprintln!("[Kiwix] Failed to parse library XML {}: {}", xml_path.display(), e),
            }
        } else {
            eprintln!("[Kiwix] Library XML file not present at {}", xml_path.display());
        }
    } else {
        eprintln!("[Kiwix] No cache_path provided, skipping library XML");
    }

    // 2) Try library.kiwix.org OPDS (paginated, has category/language)
    eprintln!("[Kiwix] Trying OPDS catalog fetch...");
    match fetch_opds_catalog() {
        Ok(entries) if !entries.is_empty() => {
            eprintln!("[Kiwix] OPDS catalog fetched: {} entries", entries.len());
            if let Some(ref path) = cache_file {
                if let Some(parent) = path.parent() {
                    match std::fs::create_dir_all(parent) {
                        Ok(()) => {}
                        Err(e) => eprintln!("[Kiwix] Failed to create cache directory {} (permissions?): {}", parent.display(), e),
                    }
                    let json = match serde_json::to_string_pretty(&entries) {
                        Ok(s) => s,
                        Err(e) => {
                            eprintln!("[Kiwix] Failed to serialize OPDS catalog to JSON: {}", e);
                            return entries;
                        }
                    };
                    match std::fs::write(path, json) {
                        Ok(()) => eprintln!("[Kiwix] Wrote OPDS cache to {}", path.display()),
                        Err(e) => eprintln!("[Kiwix] Failed to write cache file {} (permissions or storage?): {}", path.display(), e),
                    }
                }
            }
            return entries;
        }
        Ok(_entries) => eprintln!("[Kiwix] OPDS catalog returned 0 entries"),
        Err(e) => eprintln!("[Kiwix] OPDS catalog fetch failed: {}", e),
    }

    eprintln!("[Kiwix] All catalog sources failed; using hardcoded fallback (3 entries)");
    get_catalog_fallback()
}

/// Fetch full catalog from library.kiwix.org OPDS API (paginated).
/// Server returns 10 items per page; we request each page until we get none.
const OPDS_FETCH_TIMEOUT_SECS: u64 = 120;
fn fetch_opds_catalog() -> Result<Vec<KiwixEntry>, String> {
    let mut all = Vec::new();
    let mut start = 0usize;
    let timeout = std::time::Duration::from_secs(OPDS_FETCH_TIMEOUT_SECS);
    loop {
        let url = format!("{}?count={}&start={}", OPDS_ENTRIES, OPDS_PAGE_SIZE, start);
        eprintln!("[Kiwix] OPDS fetch page start={} (timeout {}s)", start, OPDS_FETCH_TIMEOUT_SECS);
        let body = ureq::get(&url)
            .set("User-Agent", OPDS_USER_AGENT)
            .set("Accept", ACCEPT_XML)
            .timeout(timeout)
            .call()
            .map_err(|e| {
                let msg = e.to_string();
                eprintln!("[Kiwix] OPDS request failed at start={}: {}", start, msg);
                msg
            })?
            .into_string()
            .map_err(|e| {
                let msg = e.to_string();
                eprintln!("[Kiwix] OPDS response read error: {}", msg);
                msg
            })?;
        let mut entries = parse_opds_feed(&body).map_err(|e| {
            eprintln!("[Kiwix] OPDS parse error at start={}: {}", start, e);
            e
        })?;
        if entries.is_empty() {
            eprintln!("[Kiwix] OPDS page at start={} returned 0 entries, stopping", start);
            break;
        }
        let n = entries.len();
        all.append(&mut entries);
        eprintln!("[Kiwix] OPDS page start={}: {} entries (total so far: {})", start, n, all.len());
        if n < OPDS_PAGE_SIZE {
            break;
        }
        start += OPDS_PAGE_SIZE;
    }
    Ok(all)
}

/// Fetch one page from the OPDS API with server-side search/filter. Uses q, lang, category,
/// maxsize (for __size_small__), start, count. Returns (entries, total) or error.
fn fetch_opds_page(
    search: &str,
    category: &str,
    language: &str,
    start: u32,
    count: u32,
) -> Result<(Vec<KiwixEntry>, u32), String> {
    let count = count.min(100).max(1);
    let mut params: Vec<String> = vec![
        format!("start={}", start),
        format!("count={}", count),
    ];
    let q = search.trim();
    if !q.is_empty() {
        params.push(format!("q={}", urlencoding::encode(q)));
    }
    if category != "all" && category != "__size_small__" && !category.is_empty() {
        params.push(format!("category={}", urlencoding::encode(category)));
    }
    if category == "__size_small__" {
        params.push("maxsize=500000000".to_string()); // 500 MB
    }
    if language != "all" && !language.is_empty() {
        params.push(format!("lang={}", urlencoding::encode(language)));
    }
    let url = format!("{}?{}", OPDS_ENTRIES, params.join("&"));
    let body = ureq::get(&url)
        .set("User-Agent", OPDS_USER_AGENT)
        .timeout(std::time::Duration::from_secs(OPDS_TIMEOUT_SECS))
        .call()
        .map_err(|e| e.to_string())?
        .into_string()
        .map_err(|e| e.to_string())?;
    let (entries, total_opt) = parse_opds_feed_with_total(&body)?;
    let total = total_opt.unwrap_or_else(|| start + entries.len() as u32);
    Ok((entries, total))
}

/// Fetch categories and languages from OPDS navigation feeds for filter dropdowns.
fn fetch_opds_facets() -> Result<KiwixFacets, String> {
    let timeout = std::time::Duration::from_secs(OPDS_TIMEOUT_SECS);
    let categories_xml = ureq::get(OPDS_CATEGORIES)
        .set("User-Agent", OPDS_USER_AGENT)
        .timeout(timeout)
        .call()
        .map_err(|e| e.to_string())?
        .into_string()
        .map_err(|e| e.to_string())?;
    let languages_xml = ureq::get(OPDS_LANGUAGES)
        .set("User-Agent", OPDS_USER_AGENT)
        .timeout(timeout)
        .call()
        .map_err(|e| e.to_string())?
        .into_string()
        .map_err(|e| e.to_string())?;
    let categories = parse_opds_navigation_titles(&categories_xml)?;
    let languages = parse_opds_language_codes(&languages_xml)?;
    Ok(KiwixFacets {
        categories,
        languages,
    })
}

/// Parse an OPDS navigation feed (e.g. categories): collect <title> from each <entry>.
fn parse_opds_navigation_titles(xml: &str) -> Result<Vec<String>, String> {
    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut list = Vec::new();
    let mut in_entry = false;
    let mut entry_depth: u32 = 0;
    let mut current_tag: Option<Vec<u8>> = None;
    let mut title = String::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Eof) => break,
            Ok(quick_xml::events::Event::Start(e)) => {
                let local: Vec<u8> = e.name().local_name().as_ref().to_vec();
                current_tag = Some(local.clone());
                if local.as_slice() == b"entry" {
                    in_entry = true;
                    entry_depth = 0;
                    title.clear();
                } else if in_entry {
                    entry_depth += 1;
                }
            }
            Ok(quick_xml::events::Event::End(e)) => {
                let local: Vec<u8> = e.name().local_name().as_ref().to_vec();
                if local.as_slice() == b"entry" {
                    in_entry = false;
                    if !title.is_empty() {
                        list.push(title.trim().to_string());
                    }
                } else if in_entry {
                    entry_depth = entry_depth.saturating_sub(1);
                }
                current_tag = None;
            }
            Ok(quick_xml::events::Event::Text(e)) => {
                if in_entry && entry_depth == 1 {
                    let text = e.unescape().unwrap_or_default().trim().to_string();
                    if current_tag.as_deref() == Some(b"title") {
                        title = text;
                    }
                }
            }
            Err(e) => return Err(e.to_string()),
            _ => {}
        }
        buf.clear();
    }
    Ok(list)
}

/// Parse OPDS languages feed: collect language code from <dc:language> in each <entry>.
fn parse_opds_language_codes(xml: &str) -> Result<Vec<String>, String> {
    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut list = Vec::new();
    let mut in_entry = false;
    let mut entry_depth: u32 = 0;
    let mut current_tag: Option<Vec<u8>> = None;
    let mut code = String::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Eof) => break,
            Ok(quick_xml::events::Event::Start(e)) => {
                let local: Vec<u8> = e.name().local_name().as_ref().to_vec();
                current_tag = Some(local.clone());
                if local.as_slice() == b"entry" {
                    in_entry = true;
                    entry_depth = 0;
                    code.clear();
                } else if in_entry {
                    entry_depth += 1;
                }
            }
            Ok(quick_xml::events::Event::End(e)) => {
                let local: Vec<u8> = e.name().local_name().as_ref().to_vec();
                if local.as_slice() == b"entry" {
                    in_entry = false;
                    if !code.is_empty() {
                        list.push(code.trim().to_lowercase());
                    }
                } else if in_entry {
                    entry_depth = entry_depth.saturating_sub(1);
                }
                current_tag = None;
            }
            Ok(quick_xml::events::Event::Text(e)) => {
                if in_entry && entry_depth == 1 {
                    let text = e.unescape().unwrap_or_default().trim().to_string();
                    // dc:language has local name "language" in dc namespace
                    if current_tag.as_deref() == Some(b"language") {
                        code = text;
                    }
                }
            }
            Err(e) => return Err(e.to_string()),
            _ => {}
        }
        buf.clear();
    }
    Ok(list)
}

/// Parse one OPDS Atom feed page; returns entries and optional totalResults (opensearch).
fn parse_opds_feed_with_total(xml: &str) -> Result<(Vec<KiwixEntry>, Option<u32>), String> {
    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut entries = Vec::new();
    let mut total_results: Option<u32> = None;
    let mut in_entry = false;
    let mut entry_depth: u32 = 0;
    let mut current_tag: Option<Vec<u8>> = None;
    let mut in_total_results = false;
    let mut title = String::new();
    let mut summary = String::new();
    let mut zim_href = String::new();
    let mut zim_length: u64 = 0;
    let mut name_id = String::new();
    let mut category = String::new();
    let mut language = String::new();
    let mut tags = String::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Eof) => break,
            Ok(quick_xml::events::Event::Start(e)) => {
                let local_name = e.name().local_name();
                let local = local_name.as_ref();
                if local == b"totalResults" {
                    in_total_results = true;
                }
                current_tag = Some(local.to_vec());
                if local == b"entry" {
                    in_entry = true;
                    entry_depth = 0;
                    title.clear();
                    summary.clear();
                    zim_href.clear();
                    zim_length = 0;
                    name_id.clear();
                    category.clear();
                    language.clear();
                    tags.clear();
                } else if in_entry {
                    entry_depth += 1;
                    if local == b"link" {
                        let mut href = String::new();
                        let mut rel = String::new();
                        let mut typ = String::new();
                        let mut length: u64 = 0;
                        for attr in e.attributes() {
                            let attr = attr.map_err(|e| e.to_string())?;
                            match attr.key.as_ref() {
                                b"href" => href = String::from_utf8_lossy(&attr.value).to_string(),
                                b"rel" => rel = String::from_utf8_lossy(&attr.value).to_string(),
                                b"type" => typ = String::from_utf8_lossy(&attr.value).to_string(),
                                b"length" => length = String::from_utf8_lossy(&attr.value).parse().unwrap_or(0),
                                _ => {}
                            }
                        }
                        if rel.contains("acquisition") && typ.contains("zim") && !href.is_empty() {
                            zim_href = href.trim_end_matches(".meta4").to_string();
                            if !zim_href.ends_with(".zim") {
                                zim_href.push_str(".zim");
                            }
                            zim_length = length;
                        }
                    }
                }
            }
            Ok(quick_xml::events::Event::End(e)) => {
                let end_name = e.name().local_name();
                if end_name.as_ref() == b"totalResults" {
                    in_total_results = false;
                }
                if end_name.as_ref() == b"entry" {
                    in_entry = false;
                    if !zim_href.is_empty() {
                        let id = if name_id.is_empty() {
                            zim_href.split('/').last().unwrap_or("").trim_end_matches(".zim").to_string()
                        } else {
                            name_id.clone()
                        };
                        entries.push(KiwixEntry {
                            id,
                            title: title.trim().to_string(),
                            description: summary.trim().to_string(),
                            url: zim_href.clone(),
                            size_bytes: zim_length,
                            size_label: format_size(zim_length),
                            category: category.trim().to_string(),
                            language: language.trim().to_string(),
                            tags: tags.trim().to_string(),
                        });
                    }
                } else if in_entry {
                    entry_depth = entry_depth.saturating_sub(1);
                }
                current_tag = None;
            }
            Ok(quick_xml::events::Event::Text(e)) => {
                if in_total_results {
                    let text = e.unescape().unwrap_or_default().trim().to_string();
                    if let Ok(n) = text.parse::<u32>() {
                        total_results = Some(n);
                    }
                } else if in_entry && entry_depth == 1 {
                    let text = e.unescape().unwrap_or_default().trim().to_string();
                    match current_tag.as_deref() {
                        Some(b"title") => title = text,
                        Some(b"summary") => summary = text,
                        Some(b"name") => name_id = text,
                        Some(b"category") => category = text,
                        Some(b"language") => language = text,
                        Some(b"tags") => tags = text,
                        _ => {}
                    }
                }
            }
            Err(e) => {
                let msg = e.to_string();
                eprintln!("[Kiwix] OPDS feed parse error (fault-tolerant: keeping {} entries so far): {}", entries.len(), msg);
                break;
            }
            _ => {}
        }
        buf.clear();
    }
    Ok((entries, total_results))
}

/// Parse one OPDS Atom feed page (entries only); used by legacy fetch_opds_catalog.
fn parse_opds_feed(xml: &str) -> Result<Vec<KiwixEntry>, String> {
    parse_opds_feed_with_total(xml).map(|(entries, _)| entries)
}

fn fetch_library_to_file(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        eprintln!("[Kiwix] Ensuring cache directory exists: {}", parent.display());
        std::fs::create_dir_all(parent).map_err(|e| {
            let msg = e.to_string();
            eprintln!("[Kiwix] Failed to create cache directory {} (permissions or invalid path?): {}", parent.display(), msg);
            msg
        })?;
    }
    let timeout = std::time::Duration::from_secs(LIBRARY_XML_TIMEOUT_SECS);
    eprintln!("[Kiwix] Fetching library XML from {} (timeout {}s)...", LIBRARY_URL, LIBRARY_XML_TIMEOUT_SECS);
    let body = ureq::get(LIBRARY_URL)
        .set("User-Agent", OPDS_USER_AGENT)
        .set("Accept", ACCEPT_XML)
        .timeout(timeout)
        .call()
        .map_err(|e| {
            let msg = e.to_string();
            eprintln!("[Kiwix] Library XML fetch failed: {}", msg);
            msg
        })
        .and_then(|r| {
            let mut reader = r.into_reader();
            let mut buf = Vec::new();
            std::io::Read::read_to_end(&mut reader, &mut buf).map_err(|e| {
                let msg = e.to_string();
                eprintln!("[Kiwix] Library XML read error: {}", msg);
                msg
            })?;
            eprintln!("[Kiwix] Downloaded {} bytes from primary", buf.len());
            Ok(buf)
        });
    let body = match body {
        Ok(b) => b,
        Err(_) => {
            eprintln!("[Kiwix] Trying mirror {} (timeout {}s)...", LIBRARY_MIRROR, LIBRARY_XML_TIMEOUT_SECS);
            ureq::get(LIBRARY_MIRROR)
                .set("User-Agent", OPDS_USER_AGENT)
                .set("Accept", ACCEPT_XML)
                .timeout(timeout)
                .call()
                .map_err(|e| {
                    let msg = e.to_string();
                    eprintln!("[Kiwix] Library XML mirror fetch failed: {}", msg);
                    msg
                })
                .and_then(|r| {
                    let mut reader = r.into_reader();
                    let mut buf = Vec::new();
                    std::io::Read::read_to_end(&mut reader, &mut buf).map_err(|e| {
                        let msg = e.to_string();
                        eprintln!("[Kiwix] Library XML mirror read error: {}", msg);
                        msg
                    })?;
                    eprintln!("[Kiwix] Downloaded {} bytes from mirror", buf.len());
                    Ok(buf)
                })?
        }
    };
    std::fs::write(path, &body).map_err(|e| {
        let msg = e.to_string();
        eprintln!("[Kiwix] Failed to write library XML to {}: {}", path.display(), msg);
        msg
    })?;
    Ok(())
}

/// Parse library XML. Fault-tolerant: skips malformed or invalid <book> nodes and logs warnings
/// instead of failing the entire parse (e.g. one bad description or missing field won't abort).
fn parse_library_xml(path: &Path) -> Result<Vec<KiwixEntry>, String> {
    let file = std::fs::File::open(path).map_err(|e| {
        eprintln!("[Kiwix] Cannot open library XML file {}: {}", path.display(), e);
        e.to_string()
    })?;
    eprintln!("[Kiwix] Parsing library XML from {}...", path.display());
    let mut reader = quick_xml::Reader::from_reader(BufReader::new(file));
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut entries = Vec::new();
    let mut book_count: u64 = 0;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Eof) => break,
            Ok(quick_xml::events::Event::Start(e)) => {
                if e.name().as_ref() == b"book" {
                    book_count += 1;
                    let mut id = String::new();
                    let mut title = String::new();
                    let mut description = String::new();
                    let mut url = String::new();
                    let mut size_bytes: u64 = 0;
                    let mut attr_error = false;
                    for attr in e.attributes() {
                        match attr {
                            Ok(a) => {
                                let key = a.key.as_ref();
                                let value = String::from_utf8_lossy(&a.value).to_string();
                                match key {
                                    b"id" => id = value,
                                    b"title" => title = value,
                                    b"description" => description = value,
                                    b"url" => {
                                        url = value.trim_end_matches(".meta4").to_string();
                                        if !url.is_empty() && !url.ends_with(".zim") {
                                            url.push_str(".zim");
                                        }
                                    }
                                    b"size" => size_bytes = value.parse().unwrap_or(0),
                                    _ => {}
                                }
                            }
                            Err(e) => {
                                eprintln!("[Kiwix] Warning: malformed attribute in <book> (skipping this attribute): {}", e);
                                attr_error = true;
                            }
                        }
                    }
                    if attr_error {
                        eprintln!("[Kiwix] Warning: book #{} had attribute errors, using parsed fields anyway", book_count);
                    }
                    if !url.is_empty() {
                        let size_label = format_size(size_bytes);
                        entries.push(KiwixEntry {
                            id: id.clone(),
                            title: title.clone(),
                            description: description.clone(),
                            url,
                            size_bytes,
                            size_label,
                            category: String::new(),
                            language: String::new(),
                            tags: String::new(),
                        });
                    } else {
                        eprintln!("[Kiwix] Warning: skipping book #{} (missing or empty url)", book_count);
                    }
                }
            }
            Err(e) => {
                let msg = e.to_string();
                eprintln!("[Kiwix] XML parse error in library file {} (fault-tolerant: keeping {} entries so far): {}", path.display(), entries.len(), msg);
                break;
            }
            _ => {}
        }
        buf.clear();
    }
    eprintln!("[Kiwix] Parsed {} entries from library XML ({} book elements seen)", entries.len(), book_count);
    Ok(entries)
}

/// Downloads a ZIM from `url` into `dest_path`, calling `on_progress(downloaded_bytes, total_bytes)` periodically.
/// `total_bytes` is None if Content-Length is missing.
pub fn download_zim(
    url: &str,
    dest_path: &Path,
    on_progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<(), String> {
    let response = ureq::get(url)
        .timeout(std::time::Duration::from_secs(3600))
        .call()
        .map_err(|e| e.to_string())?;

    let total: Option<u64> = response
        .header("Content-Length")
        .and_then(|v| v.parse().ok());

    let parent = dest_path.parent().ok_or("Invalid destination path")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut file = std::fs::File::create(dest_path).map_err(|e| e.to_string())?;

    let mut reader = response.into_reader();
    let mut buf = [0u8; 64 * 1024];
    let mut downloaded: u64 = 0;
    let mut last_emit: u64 = 0;

    loop {
        let n = std::io::Read::read(&mut reader, &mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        downloaded += n as u64;
        if downloaded - last_emit >= 256 * 1024 {
            last_emit = downloaded;
            on_progress(downloaded, total);
        }
    }
    on_progress(downloaded, total);
    Ok(())
}
