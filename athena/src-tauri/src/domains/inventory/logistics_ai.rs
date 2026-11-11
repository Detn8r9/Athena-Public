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
    let json = extract_json_object(&raw).unwrap_or(raw.trim());
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

    Ok(parsed)
}
