//! Which survival topics the library covers, and where it is thin.
//!
//! Classification is by plain keyword lists, not a model: it has to run over thousands of file
//! names in a moment, give the same answer every time, and work with no model installed.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Topic {
    Water,
    Medical,
    Food,
    Farming,
    Power,
    Vehicles,
    Making,
    Shelter,
    Comms,
    Navigation,
    Security,
    Education,
    Local,
}

pub const ALL_TOPICS: [Topic; 13] = [
    Topic::Water,
    Topic::Medical,
    Topic::Food,
    Topic::Farming,
    Topic::Power,
    Topic::Vehicles,
    Topic::Making,
    Topic::Shelter,
    Topic::Comms,
    Topic::Navigation,
    Topic::Security,
    Topic::Education,
    Topic::Local,
];

impl Topic {
    pub fn key(self) -> &'static str {
        match self {
            Topic::Water => "water",
            Topic::Medical => "medical",
            Topic::Food => "food",
            Topic::Farming => "farming",
            Topic::Power => "power",
            Topic::Vehicles => "vehicles",
            Topic::Making => "making",
            Topic::Shelter => "shelter",
            Topic::Comms => "comms",
            Topic::Navigation => "navigation",
            Topic::Security => "security",
            Topic::Education => "education",
            Topic::Local => "local",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Topic::Water => "Water and sanitation",
            Topic::Medical => "Medical",
            Topic::Food => "Food and preserving",
            Topic::Farming => "Farming and animals",
            Topic::Power => "Electricity and solar",
            Topic::Vehicles => "Vehicles and engines",
            Topic::Making => "Making and repairing things",
            Topic::Shelter => "Shelter and building",
            Topic::Comms => "Radio and communications",
            Topic::Navigation => "Maps and navigation",
            Topic::Security => "Security",
            Topic::Education => "Education",
            Topic::Local => "Local knowledge (this place, these people)",
        }
    }

    pub fn from_key(key: &str) -> Option<Topic> {
        ALL_TOPICS.iter().copied().find(|t| t.key() == key)
    }

    /// Lower-case words and phrases that mark a document as being about this topic.
    fn keywords(self) -> &'static [&'static str] {
        match self {
            Topic::Water => &[
                "water", "purif", "filtration", "filter", "well", "rainwater", "sanitation", "latrine", "sewage", "chlorin",
                "cistern", "hand pump", "distill", "hygiene", "greywater",
            ],
            Topic::Medical => &[
                "medical", "medicine", "first aid", "doctor", "nurse", "nursing", "wound", "fracture", "dental", "dentist",
                "midwife", "midwives", "childbirth", "pharmac", "antibiotic", "anatomy", "surgery", "triage", "disease",
                "infection", "cpr", "health", "clinic", "dosage", "symptom",
            ],
            Topic::Food => &[
                "canning", "preserv", "food storage", "recipe", "cooking", "smoking meat", "dehydrat", "ferment", "pickl",
                "nutrition", "foraging", "edible", "butcher", "bread", "ration", "root cellar", "home canning",
            ],
            Topic::Farming => &[
                "garden", "farming", "agricultur", "seed", "crop", "soil", "compost", "livestock", "poultry", "chicken",
                "goat", "cattle", "beekeep", "irrigation", "greenhouse", "orchard", "veterinar", "animal husbandry", "permaculture",
            ],
            Topic::Power => &[
                "solar", "photovoltaic", "battery", "batteries", "inverter", "charge controller", "generator", "electric",
                "wiring", "off-grid", "off grid", "wind turbine", "hydro", "voltage", "circuit", "electronics", "alternator",
            ],
            Topic::Vehicles => &[
                "vehicle", "engine", "diesel", "automotive", "car repair", "truck", "motorcycle", "tractor", "transmission",
                "carburet", "service manual", "repair manual", "owner's manual", "owners manual", "brake", "bicycle", "small engine", "mechanic",
            ],
            Topic::Making => &[
                "blacksmith", "forge", "welding", "metalwork", "machining", "lathe", "woodwork", "carpentry", "joinery",
                "soap", "tanning", "leather", "textile", "weaving", "sewing", "pottery", "cement", "concrete", "lime", "charcoal",
                "glue", "rope", "tool making", "foundry", "casting", "workshop", "repair", "fabricat",
            ],
            Topic::Shelter => &[
                "shelter", "construction", "building", "masonry", "roofing", "insulation", "plumbing", "stove", "heating",
                "adobe", "log cabin", "timber frame", "earthbag", "chimney", "firewood", "survival", "camping", "camp-lore",
                "woodcraft", "bushcraft", "outdoors", "knots", "scout",
            ],
            Topic::Comms => &[
                "radio", "antenna", "ham ", "amateur radio", "morse", "meshtastic", "lora", "transceiver", "frequency",
                "repeater", "shortwave", "signal", "communications",
            ],
            Topic::Navigation => &[
                "map", "navigation", "compass", "topograph", "orienteering", "gps", "atlas", "chart", "land nav", "celestial",
            ],
            Topic::Security => &[
                "security", "defense", "defence", "perimeter", "patrol", "firearm", "self-defense", "self defence", "tactic",
                "opsec", "evasion", "sentry", "army", "military", "preparedness", "disaster",
            ],
            Topic::Education => &[
                "textbook", "curriculum", "lesson", "algebra", "mathematics", "physics", "chemistry", "biology", "history",
                "reader", "grammar", "phonics", "school", "teaching", "geometry", "arithmetic", "encyclopedia", "wikipedia",
                "vikidia", "phet", "science", "khan academy",
            ],
            Topic::Local => &[
                "cache", "stash", "supply list", "supplies list", "inventory list", "contact list", "rendezvous", "rally point",
                "bug out", "bug-out", "evacuation plan", "emergency plan", "site plan", "floor plan", "property map",
                "well log", "neighbors", "neighbours", "hidden", "coordinates", "meeting point", "safe house",
            ],
        }
    }
}

/// Words that suggest a how-to or reference document on any topic.
const REFERENCE_WORDS: &[&str] = &[
    "manual", "handbook", "guide", "how to", "how-to", "field manual", "instructions", "reference", "procedures",
    "maintenance", "troubleshoot", "survival", "emergency", "preparedness", "diy", "encyclopedia", "tutorial",
];

fn count_hits(haystack: &str, words: &[&str]) -> u32 {
    words.iter().filter(|w| haystack.contains(*w)).count() as u32
}

/// How strongly `name` (file name or title) and `sample` (some of the text) point at each topic.
/// A match in the name counts three times: names are short and deliberate.
pub fn score_topics(name: &str, sample: &str) -> Vec<(Topic, u32)> {
    let name = name.to_lowercase().replace(['_', '-', '.'], " ");
    let sample = sample.to_lowercase();
    let mut out: Vec<(Topic, u32)> = ALL_TOPICS
        .iter()
        .map(|t| (*t, 3 * count_hits(&name, t.keywords()) + count_hits(&sample, t.keywords())))
        .filter(|(_, s)| *s > 0)
        .collect();
    out.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    out
}

/// The topic a document is mostly about, if any.
pub fn best_topic(name: &str, sample: &str) -> Option<Topic> {
    score_topics(name, sample).first().map(|(t, _)| *t)
}

/// A rough 0-100 usefulness score for a document found on a disk.
pub fn usefulness(name: &str, sample: &str) -> u32 {
    let topics = score_topics(name, sample);
    let top = topics.first().map(|(_, s)| *s).unwrap_or(0);
    let lower_name = name.to_lowercase();
    let reference = 3 * count_hits(&lower_name, REFERENCE_WORDS) + count_hits(&sample.to_lowercase(), REFERENCE_WORDS);
    (top * 8 + reference * 6 + (topics.len() as u32).min(4) * 3).min(100)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    None,
    Thin,
    Some,
    Good,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    pub title: &'static str,
    /// Folder under `download.kiwix.org/zim/`.
    pub folder: &'static str,
    /// File name without the release date; the newest release is looked up when downloading.
    pub base: &'static str,
    pub approx_size: &'static str,
    pub why: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopicCoverage {
    pub topic: Topic,
    pub label: &'static str,
    pub level: Level,
    /// Books and documents on this topic.
    pub documents: u32,
    /// Offline encyclopedias and packs on this topic.
    pub archives: u32,
    pub examples: Vec<String>,
    pub suggestions: Vec<Suggestion>,
}

/// Sources checked against the Kiwix listings on 12 November 2026. Sizes are approximate.
pub fn suggestions_for(topic: Topic) -> Vec<Suggestion> {
    let s = |title, folder, base, approx_size, why| Suggestion { title, folder, base, approx_size, why };
    match topic {
        Topic::Vehicles => vec![
            s("Car and engine repair questions and answers", "stack_exchange", "mechanics.stackexchange.com_en_all", "330 MB", "Diagnosing and fixing cars, trucks and small engines."),
            s("iFixit repair guides", "ifixit", "ifixit_en_all", "3.3 GB", "Step-by-step repair guides with pictures for vehicles, tools and electronics."),
        ],
        Topic::Power => vec![
            s("Electronics questions and answers", "stack_exchange", "electronics.stackexchange.com_en_all", "4 GB", "Circuits, batteries, inverters and repairs."),
            s("Home improvement questions and answers", "stack_exchange", "diy.stackexchange.com_en_all", "1.9 GB", "House wiring, panels, plumbing and repairs."),
            s("Energypedia", "other", "energypedia_en_all_nopic", "690 MB", "Off-grid solar, small hydro, stoves and batteries."),
        ],
        Topic::Making => vec![
            s("Woodworking questions and answers", "stack_exchange", "woodworking.stackexchange.com_en_all", "100 MB", "Joinery, tools and finishing."),
            s("Engineering questions and answers", "stack_exchange", "engineering.stackexchange.com_en_all", "250 MB", "Materials, structures and machines."),
            s("Wikibooks", "wikibooks", "wikibooks_en_all_nopic", "3.3 GB", "Open textbooks and how-to books on trades and crafts."),
            s("Appropedia", "other", "appropedia_en_all_maxi", "560 MB", "Appropriate technology you can build."),
        ],
        Topic::Farming => vec![
            s("Gardening questions and answers", "stack_exchange", "gardening.stackexchange.com_en_all", "890 MB", "Growing food, soil, pests and seed saving."),
        ],
        Topic::Food => vec![
            s("Cooking questions and answers", "stack_exchange", "cooking.stackexchange.com_en_all", "230 MB", "Cooking, preserving and food safety."),
        ],
        Topic::Comms => vec![
            s("Amateur radio questions and answers", "stack_exchange", "ham.stackexchange.com_en_all", "75 MB", "Antennas, licences and operating."),
        ],
        Topic::Shelter => vec![
            s("Home improvement questions and answers", "stack_exchange", "diy.stackexchange.com_en_all", "1.9 GB", "Building, roofing, heating and plumbing."),
            s("Outdoors questions and answers", "stack_exchange", "outdoors.stackexchange.com_en_all", "135 MB", "Camping, shelter and fire."),
        ],
        Topic::Water => vec![s("Water treatment collection", "other", "zimgit-water_en", "20 MB", "Treating and storing drinking water.")],
        Topic::Medical => vec![s("Wikipedia medicine", "wikipedia", "wikipedia_en_medicine_nopic", "1 GB", "Medical encyclopedia without pictures.")],
        Topic::Education => vec![
            s("Vikidia", "vikidia", "vikidia_en_all_nopic", "10 MB", "Encyclopedia written for children."),
            s("Full Wikipedia without pictures", "wikipedia", "wikipedia_en_all_nopic", "49 GB", "General reference for every topic."),
        ],
        Topic::Navigation | Topic::Security | Topic::Local => vec![],
    }
}

/// One library item to count: its title or file name, and whether it is a whole archive.
pub struct LibraryItem {
    pub name: String,
    pub is_archive: bool,
}

/// Coverage of every topic from the names of what the library holds.
pub fn build_coverage(items: &[LibraryItem]) -> Vec<TopicCoverage> {
    ALL_TOPICS
        .iter()
        .map(|topic| {
            let mut documents = 0u32;
            let mut archives = 0u32;
            let mut examples = Vec::new();
            for item in items {
                let scored = score_topics(&item.name, "");
                let Some((_, top)) = scored.first() else { continue };
                // Count a document under every topic it scores nearly as well for.
                if !scored.iter().any(|(t, s)| t == topic && *s * 2 >= *top) {
                    continue;
                }
                if item.is_archive {
                    archives += 1;
                } else {
                    documents += 1;
                }
                if examples.len() < 4 {
                    examples.push(item.name.clone());
                }
            }
            // One whole archive is worth many single documents.
            let weight = documents + archives * 8;
            let level = match weight {
                0 => Level::None,
                1..=3 => Level::Thin,
                4..=11 => Level::Some,
                _ => Level::Good,
            };
            TopicCoverage {
                topic: *topic,
                label: topic.label(),
                level,
                documents,
                archives,
                examples,
                suggestions: if matches!(level, Level::None | Level::Thin) { suggestions_for(*topic) } else { Vec::new() },
            }
        })
        .collect()
}

/// Topics with little or nothing in the library.
pub fn blind_spots(coverage: &[TopicCoverage]) -> Vec<Topic> {
    coverage.iter().filter(|c| matches!(c.level, Level::None | Level::Thin)).map(|c| c.topic).collect()
}

/// Names of everything in the library, for [`build_coverage`].
/// Document types worth counting when they sit on a shelf but are not indexed yet.
const SHELF_EXTS: &[&str] = &["pdf", "txt", "md", "epub", "doc", "docx", "rtf", "odt", "html", "htm"];

fn item_name(title: &str, file: &str) -> String {
    let stem = file.rsplit_once('.').map(|(s, _)| s).unwrap_or(file);
    if title.trim().is_empty() || title == file || title == stem {
        file.to_string()
    } else {
        format!("{title} {file}")
    }
}

/// Everything this station holds: indexed documents, files still waiting on a shelf, and every
/// ZIM archive the reader can serve (encyclopedias are read in place, never indexed per article).
pub fn library_items(db: &crate::db::Database, data_root: &std::path::Path) -> Vec<LibraryItem> {
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<LibraryItem> = indexed_items(db)
        .into_iter()
        .filter(|(_, file)| seen.insert(file.to_lowercase()))
        .map(|(item, _)| item)
        .collect();
    let roots = crate::kiwix_serve::zim_discovery_roots(data_root);
    for path in crate::kiwix_serve::collect_zim_paths_from_roots(&roots) {
        let file = path.rsplit('/').next().unwrap_or(&path).to_string();
        if seen.insert(file.to_lowercase()) {
            out.push(LibraryItem { name: file, is_archive: true });
        }
    }
    let shelves = crate::athena_paths::AthenaPaths::new(data_root).user_library_dir();
    for e in walkdir::WalkDir::new(&shelves)
        .max_depth(4)
        .into_iter()
        .filter_entry(|e| {
            let n = e.file_name().to_string_lossy();
            !n.starts_with('.') && n != "group-memory"
        })
        .flatten()
        .filter(|e| e.file_type().is_file())
    {
        let file = e.file_name().to_string_lossy().into_owned();
        let ext = file.rsplit_once('.').map(|(_, x)| x.to_ascii_lowercase()).unwrap_or_default();
        if SHELF_EXTS.contains(&ext.as_str()) && seen.insert(file.to_lowercase()) {
            out.push(LibraryItem { name: file, is_archive: false });
        }
    }
    out
}

fn indexed_items(db: &crate::db::Database) -> Vec<(LibraryItem, String)> {
    db.with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT COALESCE(NULLIF(display_title, ''), name), path, file_type FROM indexed_files
                 WHERE path NOT LIKE '%.zim::%' AND path NOT LIKE '%/group-memory/%' LIMIT 200000",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        let mut zims = std::collections::HashSet::new();
        for (title, path, file_type) in rows.flatten() {
            let file = path.rsplit('/').next().unwrap_or(&path).to_string();
            let is_archive = file_type.contains("zim") || file.ends_with(".zim");
            if is_archive && !zims.insert(file.clone()) {
                continue;
            }
            out.push((LibraryItem { name: item_name(&title, &file), is_archive }, file));
        }
        // ZIM articles are stored as `<archive>.zim::<article>`; count each archive once.
        let mut stmt = conn
            .prepare("SELECT DISTINCT substr(path, 1, instr(path, '.zim::') + 3) FROM indexed_files WHERE path LIKE '%.zim::%'")
            .map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0)).map_err(|e| e.to_string())?;
        for path in rows.flatten() {
            let file = path.rsplit('/').next().unwrap_or(&path).to_string();
            if zims.insert(file.clone()) {
                out.push((LibraryItem { name: file.clone(), is_archive: true }, file));
            }
        }
        Ok(out)
    })
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(name: &str) -> LibraryItem {
        LibraryItem { name: name.to_string(), is_archive: false }
    }

    #[test]
    fn names_point_at_the_right_topic() {
        assert_eq!(best_topic("Where There Is No Doctor.pdf", ""), Some(Topic::Medical));
        assert_eq!(best_topic("toyota_hilux_service_manual.pdf", ""), Some(Topic::Vehicles));
        assert_eq!(best_topic("USDA Complete Guide to Home Canning", ""), Some(Topic::Food));
        assert_eq!(best_topic("holiday photos list.txt", ""), None);
        assert_eq!(best_topic("notes.txt", "The supply cache is hidden behind the north well, coordinates below."), Some(Topic::Local));
    }

    #[test]
    fn common_survival_titles_find_a_topic() {
        assert_eq!(best_topic("Woodcraft and Camping (Nessmuk).txt", ""), Some(Topic::Shelter));
        assert_eq!(best_topic("vikidia_en_all_nopic_2026-09a.zim", ""), Some(Topic::Education));
        assert_eq!(best_topic("phet_en_all_2026-08.zim", ""), Some(Topic::Education));
        assert!(best_topic("US Army Survival Manual FM 21-76 (1992).pdf", "").is_some());
        assert!(best_topic("FEMA Are You Ready - Guide to Citizen Preparedness.pdf", "").is_some());
    }

    #[test]
    fn a_manual_scores_higher_than_an_unrelated_file() {
        assert!(usefulness("diesel engine repair manual.pdf", "") > usefulness("meeting minutes.docx", ""));
        assert_eq!(usefulness("IMG_2041.txt", "lorem ipsum"), 0);
    }

    #[test]
    fn coverage_marks_missing_topics_and_offers_sources() {
        let items = vec![
            doc("Where There Is No Doctor"),
            doc("Where There Is No Dentist"),
            doc("A Book for Midwives"),
            doc("Clinical Nursing Skills"),
            LibraryItem { name: "wikipedia_en_medicine_nopic_2026-10.zim".to_string(), is_archive: true },
        ];
        let cov = build_coverage(&items);
        let get = |t: Topic| cov.iter().find(|c| c.topic == t).unwrap();
        assert_eq!(get(Topic::Medical).level, Level::Good);
        assert!(get(Topic::Medical).suggestions.is_empty());
        assert_eq!(get(Topic::Vehicles).level, Level::None);
        assert!(get(Topic::Vehicles).suggestions.iter().any(|s| s.base.starts_with("mechanics")));
        assert!(blind_spots(&cov).contains(&Topic::Vehicles));
        assert!(!blind_spots(&cov).contains(&Topic::Medical));
    }
}
