use crate::config::{AppConfig, CustomTheme};

#[derive(Debug, Clone)]
pub struct ThemePreset {
    pub id: &'static str,
    pub name: &'static str,
    pub personality_id: &'static str,
    /// Copy for theme picker / tooltips when exposed from the shell.
    #[allow(dead_code)]
    pub description: &'static str,
}

const PRESETS: &[ThemePreset] = &[
    ThemePreset {
        id: "default",
        name: "Default survival guide",
        personality_id: "default",
        description: "Calm, practical survival instructor with clear, direct answers.",
    },
    ThemePreset {
        id: "fallout",
        name: "Fallout terminal",
        personality_id: "fallout",
        description: "Dry, retro-futuristic terminal persona with dark humor and concise guidance.",
    },
    ThemePreset {
        id: "fieldguide",
        name: "Old survival manual",
        personality_id: "fieldguide",
        description: "Terse, instructional voice like a vintage field guide or army manual; numbered steps, short sentences, no-nonsense.",
    },
];

#[allow(dead_code)] // For future theme-picker commands / sync with the frontend
pub fn list_theme_presets() -> &'static [ThemePreset] {
    PRESETS
}

pub fn get_active_theme(config: &AppConfig) -> ThemePreset {
    let theme_id = config
        .theme_id
        .as_deref()
        .or_else(|| config.personality_id.as_deref())
        .unwrap_or("default");
    PRESETS
        .iter()
        .find(|p| p.id == theme_id)
        .cloned()
        .unwrap_or(PRESETS[0].clone())
}

pub fn get_active_personality_id(config: &AppConfig) -> String {
    if let Some(ref p) = config.personality_id {
        return p.clone();
    }
    let theme = get_active_theme(config);
    theme.personality_id.to_string()
}

#[allow(dead_code)] // For future custom-theme management in settings
pub fn list_custom_themes(config: &AppConfig) -> &[CustomTheme] {
    &config.custom_themes
}

