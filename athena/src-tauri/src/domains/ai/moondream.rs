//! Local camera / attachment vision helpers for chat.
//! Images stay on-device; Ollama vision models (llava, moondream, etc.) describe them locally.

use crate::ollama::{chat, get_available_models, ChatMessage, ModelOptions};

const VISION_NAME_HINTS: &[&str] = &[
    "llava",
    "moondream",
    "bakllava",
    "minicpm-v",
    "minicpm_v",
    "qwen2-vl",
    "qwen2.5-vl",
    "qwen3-vl",
    "gemma3",
    "llama3.2-vision",
    "llama4",
    "vision",
];

/// True when a model name looks vision-capable (Ollama multimodal tags).
pub fn name_is_vision_capable(name: &str) -> bool {
    let pl = name.to_lowercase();
    VISION_NAME_HINTS.iter().any(|h| pl.contains(h))
}

/// Pick a locally installed Ollama model that is likely to accept `images`.
pub fn pick_vision_model(preferred: Option<&str>) -> Option<String> {
    let models = get_available_models(None).ok()?;
    if let Some(p) = preferred.map(str::trim).filter(|s| !s.is_empty()) {
        if models.iter().any(|m| m.name == p) && name_is_vision_capable(p) {
            return Some(p.to_string());
        }
    }
    for hint in VISION_NAME_HINTS {
        if let Some(m) = models.iter().find(|m| m.name.to_lowercase().contains(hint)) {
            return Some(m.name.clone());
        }
    }
    None
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct VisionCapability {
    pub ready: bool,
    pub model: Option<String>,
}

pub fn vision_capability(preferred_chat_model: Option<&str>) -> VisionCapability {
    let preferred = preferred_chat_model
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .and_then(|p| {
            if name_is_vision_capable(p) {
                let models = get_available_models(None).ok()?;
                if models.iter().any(|m| m.name == p) {
                    return Some(p.to_string());
                }
            }
            None
        });
    let model = preferred.or_else(|| pick_vision_model(None));
    VisionCapability {
        ready: model.is_some(),
        model,
    }
}

/// Describe what is in the frame(s) using a local vision model. Returns plain text for prompt injection.
pub fn describe_images_locally(
    images_b64: &[String],
    user_question: &str,
) -> Result<String, String> {
    if images_b64.is_empty() {
        return Err("No images provided.".to_string());
    }
    let model = pick_vision_model(None).ok_or_else(|| {
        "No local vision model found. Install one with Ollama (for example `ollama pull moondream` or `ollama pull llava`) to enable camera object ID.".to_string()
    })?;

    let q = user_question.trim();
    let prompt = if q.is_empty() {
        "Describe what you see in this camera frame. Identify people, objects, text, and anything useful for a survival / field assistant. Be concise and concrete.".to_string()
    } else {
        format!(
            "The user asked: \"{q}\"\n\nLook at the camera frame. Identify any objects the user may be holding up, plus relevant scene context. Answer in a short factual visual inventory the assistant can use (objects, labels/text, condition, hazards)."
        )
    };

    let messages = vec![ChatMessage {
        role: "user".to_string(),
        content: prompt,
        images: Some(images_b64.to_vec()),
    }];
    let opts = Some(ModelOptions {
        temperature: 0.2,
        num_ctx: 2048,
        num_predict: None,
    });
    let desc = chat(None, &model, messages, opts)?;
    let trimmed = desc.trim().to_string();
    if trimmed.is_empty() {
        return Err("Vision model returned an empty description.".to_string());
    }
    Ok(format!(
        "[Camera vision — local model `{model}`]\n{trimmed}"
    ))
}
