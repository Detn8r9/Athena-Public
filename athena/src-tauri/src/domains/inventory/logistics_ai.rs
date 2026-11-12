use serde::{Deserialize, Serialize};

use crate::config::AppConfig;
use crate::inference;
use crate::ollama::{ChatMessage, ModelInfo};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogisticsAiInventoryLine {
    /// add_item | delta_quantity | set_quantity (legacy absolute)
    #[serde(default = "default_action")]
    pub action: String,
    /// One of: Infrastructure, Vanguard, Sustenance, InformationMedical — or empty to use submitter
    #[serde(default, alias = "division")]
    pub directorate: String,
    /// The prompt asks for `item_name`; without the alias every line failed to parse and the
    /// whole answer fell back to "General, no inventory", so notes never changed stock.
    #[serde(alias = "item_name")]
    pub item_name: String,
    pub category: String,
    /// Signed delta: +5 found, -3 used; for set_quantity this is the absolute new total
    pub quantity: i64,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub confidence: Option<f64>,
}

fn default_action() -> String {
    "delta_quantity".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogisticsAiResult {
    pub section: String,
    pub kind: String,
    #[serde(default)]
    pub inventory: Vec<LogisticsAiInventoryLine>,
}

fn default_result() -> LogisticsAiResult {
    LogisticsAiResult {
        section: "General".to_string(),
        kind: "log_note".to_string(),
        inventory: Vec::new(),
    }
}

fn extract_json_object(s: &str) -> Option<&str> {
    let start = s.find('{')?;
    let end = s.rfind('}')?;
    if end <= start {
        return None;
    }
    Some(&s[start..=end])
}

pub fn classify_logistics_message(
    config: &AppConfig,
    ollama_models: &[ModelInfo],
    message: &str,
) -> Result<LogisticsAiResult, String> {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return Ok(default_result());
    }

    // Routine notes ("Used 2 bandages") are read by plain rules: instant, and not at the mercy of
    // a small local model, which missed them.
    if let Some(result) = parse_simple_inventory_note(trimmed) {
        return Ok(result);
    }

    let system = r#"You are an assistant that classifies logistics log messages and extracts inventory *changes* (deltas) when clearly stated.

Return STRICT JSON only. No prose, no markdown, no code fences.

Schema:
{
  "section": "Inventory|Requests|Deliveries|LossDamage|Maintenance|General|Other",
  "kind": "log_note|inventory|both",
  "inventory": [
    {
      "action": "delta_quantity|add_item|set_quantity",
      "directorate": "Infrastructure|Vanguard|Sustenance|InformationMedical|\"\"",
      "item_name": "<canonical item name>",
      "category": "food|water|weapons|medical|equipment|comms|fuel|materials|general",
      "quantity": <signed integer: positive adds to stock, negative removes>,
      "location": "<place name e.g. Anytown; optional; empty string if unknown>",
      "confidence": <number 0..1 or null>
    }
  ]
}

Add verbs (imply positive delta): found, added, received, delivered, recovered, restocked, built.
Remove verbs (imply negative delta): used, consumed, lost, depleted, spent, issued, handed out, damaged, destroyed.

Use directorate= empty string to mean "same as the submitter's directorate" when not stated.
Use add_item when the user clearly introduces a new kind of item not already tracked; use delta_quantity when changing counts of an existing or clearly referenced item. Use set_quantity only for explicit absolute counts ("set X to 12" / "we have exactly 8 left").

If no inventory change is clear, return kind=log_note and inventory=[].
If uncertain or ambiguous, return kind=log_note and inventory=[].
Example: "Found 5 bottles of fresh water by the lake in Anytown" -> delta_quantity, +5, item "Bottle of water", water, location "Anytown".
If category is unclear, use "general".
"#;

    let user = format!("MESSAGE:\n{trimmed}\n");
    let messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: system.to_string(),
        images: None,
        },
        ChatMessage {
            role: "user".to_string(),
            content: user,
        images: None,
        },
    ];

    let raw = inference::chat_unified(config, ollama_models, messages, None)?;
    Ok(parse_logistics_ai(&raw))
}

const REMOVE_VERBS: &[&str] = &[
    "used", "consumed", "ate", "drank", "lost", "spent", "issued", "gave", "handed out", "took", "burned", "burnt", "broke", "destroyed",
];
const ADD_VERBS: &[&str] = &[
    "found", "added", "got", "received", "bought", "picked up", "collected", "stored", "brought", "recovered", "restocked", "made", "harvested",
];

fn number_from(word: &str) -> Option<i64> {
    if let Ok(n) = word.parse::<i64>() {
        return (n > 0 && n < 1_000_000).then_some(n);
    }
    const WORDS: [&str; 13] = ["zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven", "twelve"];
    match word {
        "a" | "an" => Some(1),
        w => WORDS.iter().position(|x| *x == w).filter(|n| *n > 0).map(|n| n as i64),
    }
}

fn category_for(item: &str) -> &'static str {
    let t = item.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| t.contains(w));
    if has(&["water"]) {
        "water"
    } else if has(&["bandage", "gauze", "aspirin", "ibuprofen", "antibiotic", "medic", "pill", "tablet", "syringe", "splint", "first aid"]) {
        "medical"
    } else if has(&["rice", "bean", "flour", "can of", "cans of", "ration", "meal", "food", "oat", "pasta", "salt", "sugar"]) {
        "food"
    } else if has(&["petrol", "gasoline", "diesel", "fuel", "propane", "kerosene"]) {
        "fuel"
    } else if has(&["battery", "batteries", "radio", "antenna"]) {
        "comms"
    } else if has(&["ammo", "round", "cartridge", "rifle", "pistol"]) {
        "weapons"
    } else {
        "general"
    }
}

/// Read a routine stock note without the model: "Used 2 bandages", "Found five cans of beans and
/// 3 bottles of water at the depot". Returns `None` for anything it does not fully understand,
/// which then goes to the model as before.
pub fn parse_simple_inventory_note(message: &str) -> Option<LogisticsAiResult> {
    let text = message.trim().trim_end_matches(|c: char| c == '.' || c == '!');
    if text.is_empty() || text.len() > 200 || text.contains('?') || text.contains('\n') {
        return None;
    }
    let lower = text.to_lowercase();
    let (sign, rest) = REMOVE_VERBS
        .iter()
        .map(|v| (-1i64, *v))
        .chain(ADD_VERBS.iter().map(|v| (1i64, *v)))
        .find_map(|(sign, verb)| {
            let after = lower.strip_prefix(verb).or_else(|| lower.strip_prefix(&format!("we {verb}")).or_else(|| lower.strip_prefix(&format!("i {verb}"))))?;
            after.starts_with(' ').then(|| (sign, &text[text.len() - after.len()..]))
        })?;

    // "… at the depot" / "… from the shed": a place, kept out of the item name.
    let (items_part, location) = [" at ", " from ", " in ", " near ", " by "]
        .iter()
        .filter_map(|k| rest.to_lowercase().find(k).map(|i| (i, k.len())))
        .min_by_key(|(i, _)| *i)
        .map(|(i, klen)| {
            let place = rest[i + klen..].trim().trim_start_matches("the ").trim();
            (&rest[..i], (!place.is_empty()).then(|| place.to_string()))
        })
        .unwrap_or((rest, None));

    let mut lines = Vec::new();
    for clause in items_part.split(|c| c == ',' || c == ';').flat_map(|c| c.split(" and ")) {
        let clause = clause.trim();
        if clause.is_empty() {
            continue;
        }
        let (first, name) = clause.split_once(' ')?;
        let quantity = number_from(&first.to_lowercase())?;
        let name = name.trim().trim_start_matches("of ").trim_start_matches("the ").trim();
        if name.is_empty() || name.split_whitespace().count() > 6 {
            return None;
        }
        let mut item = name.to_string();
        if let Some(first_char) = item.get(..1) {
            item = format!("{}{}", first_char.to_uppercase(), &item[1..]);
        }
        lines.push(LogisticsAiInventoryLine {
            action: "delta_quantity".to_string(),
            directorate: String::new(),
            category: category_for(&item).to_string(),
            item_name: item,
            quantity: sign * quantity,
            location: location.clone(),
            confidence: Some(0.9),
        });
    }
    if lines.is_empty() {
        return None;
    }
    Some(LogisticsAiResult {
        section: if sign < 0 { "Inventory" } else { "Deliveries" }.to_string(),
        kind: "inventory".to_string(),
        inventory: lines,
    })
}

#[cfg(test)]
mod simple_note_tests {
    use super::*;

    #[test]
    fn used_two_bandages_removes_two() {
        let r = parse_simple_inventory_note("Used 2 bandages").unwrap();
        assert_eq!(r.inventory.len(), 1);
        assert_eq!(r.inventory[0].item_name, "Bandages");
        assert_eq!(r.inventory[0].quantity, -2);
        assert_eq!(r.inventory[0].category, "medical");
    }

    #[test]
    fn several_items_and_a_place() {
        let r = parse_simple_inventory_note("We found five cans of beans and 3 bottles of water at the depot.").unwrap();
        assert_eq!(r.inventory.len(), 2);
        assert_eq!((r.inventory[0].item_name.as_str(), r.inventory[0].quantity), ("Cans of beans", 5));
        assert_eq!((r.inventory[1].item_name.as_str(), r.inventory[1].quantity), ("Bottles of water", 3));
        assert_eq!(r.inventory[1].location.as_deref(), Some("depot"));
    }

    #[test]
    fn unclear_notes_go_to_the_model() {
        assert!(parse_simple_inventory_note("Generator is making a strange noise").is_none());
        assert!(parse_simple_inventory_note("Used some bandages").is_none());
        assert!(parse_simple_inventory_note("Did we use 2 bandages?").is_none());
        assert!(parse_simple_inventory_note("Found the missing radio").is_none());
    }
}

/// Turn the model's reply into a cleaned result (falls back to a plain log note).
pub fn parse_logistics_ai(raw: &str) -> LogisticsAiResult {
    let json = extract_json_object(raw).unwrap_or(raw.trim());
    let mut parsed: LogisticsAiResult =
        serde_json::from_str(json).unwrap_or_else(|_| default_result());

    parsed.section = match parsed.section.trim() {
        "Inventory" | "Requests" | "Deliveries" | "LossDamage" | "Maintenance" | "General" | "Other" => {
            parsed.section.trim().to_string()
        }
        _ => "General".to_string(),
    };

    parsed.kind = match parsed.kind.trim() {
        "log_note" | "inventory" | "both" => parsed.kind.trim().to_string(),
        _ => {
            if parsed.inventory.is_empty() {
                "log_note".to_string()
            } else {
                "inventory".to_string()
            }
        }
    };

    parsed.inventory = parsed
        .inventory
        .into_iter()
        .filter_map(|mut l| {
            let item = l.item_name.trim().to_string();
            if item.is_empty() {
                return None;
            }
            let a = l.action.to_ascii_lowercase();
            l.action = match a.as_str() {
                "add_item" => "add_item".to_string(),
                "set_quantity" => "set_quantity".to_string(),
                "delta_quantity" => "delta_quantity".to_string(),
                "set" => "set_quantity".to_string(),
                _ => "delta_quantity".to_string(),
            };
            l.directorate = l.directorate.trim().to_string();
            l.category = match l.category.trim().to_lowercase().as_str() {
                "food" | "water" | "weapons" | "medical" | "equipment" | "comms" | "fuel"
                | "materials" => l.category.trim().to_lowercase(),
                _ => "general".to_string(),
            };
            l.item_name = item;
            if l.quantity == 0 {
                return None;
            }
            l.location = l
                .location
                .as_ref()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            if let Some(c) = l.confidence {
                if !(0.0..=1.0).contains(&c) {
                    l.confidence = None;
                }
            }
            Some(l)
        })
        .collect();

    if parsed.inventory.is_empty() {
        parsed.kind = "log_note".to_string();
    }

    parsed
}

#[cfg(test)]
mod tests {
    use super::parse_logistics_ai;

    #[test]
    fn a_reply_in_the_prompt_schema_is_understood() {
        let raw = r#"{"section":"Inventory","kind":"inventory","inventory":[{"action":"delta_quantity","directorate":"","item_name":"Bandage","category":"medical","quantity":-2,"location":"clinic","confidence":0.9}]}"#;
        let r = parse_logistics_ai(raw);
        assert_eq!(r.section, "Inventory");
        assert_eq!(r.inventory.len(), 1);
        assert_eq!(r.inventory[0].item_name, "Bandage");
        assert_eq!(r.inventory[0].quantity, -2);
    }

    #[test]
    fn camel_case_and_surrounding_prose_still_parse() {
        let raw = "Here you go:\n{\"section\":\"Deliveries\",\"kind\":\"both\",\"inventory\":[{\"itemName\":\"Rice\",\"category\":\"food\",\"quantity\":10}]}";
        let r = parse_logistics_ai(raw);
        assert_eq!(r.inventory.len(), 1);
        assert_eq!(r.inventory[0].item_name, "Rice");
    }

    #[test]
    fn nonsense_becomes_a_plain_note() {
        let r = parse_logistics_ai("I am not sure");
        assert_eq!(r.kind, "log_note");
        assert!(r.inventory.is_empty());
    }
}
