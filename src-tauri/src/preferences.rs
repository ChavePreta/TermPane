use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const FILENAME: &str = "preferences.json";
const APP_DIR: &str = "TermPane";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preferences {
    #[serde(default = "default_schema")]
    pub schema_version: u32,
    #[serde(default = "default_font_family")]
    pub font_family: String,
    #[serde(default = "default_font_size")]
    pub font_size: u32,
    #[serde(default = "default_line_height")]
    pub line_height: f32,
    #[serde(default = "default_cursor_blink")]
    pub cursor_blink: bool,
    #[serde(default = "default_bell_notifications")]
    pub bell_notifications: bool,
    #[serde(default = "default_restore_sessions")]
    pub restore_sessions: bool,
    /// How clickable file paths open: a command with `{file}`, `{line}` and
    /// `{col}` placeholders; empty means `code -g`, then the default app.
    #[serde(default)]
    pub editor_command: String,
}

fn default_schema() -> u32 {
    1
}
fn default_font_family() -> String {
    "Menlo, Monaco, \"JetBrains Mono\", \"Fira Code\", Consolas, monospace".to_string()
}
fn default_font_size() -> u32 {
    13
}
fn default_line_height() -> f32 {
    1.15
}
fn default_cursor_blink() -> bool {
    true
}
fn default_bell_notifications() -> bool {
    true
}
fn default_restore_sessions() -> bool {
    true
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            schema_version: default_schema(),
            font_family: default_font_family(),
            font_size: default_font_size(),
            line_height: default_line_height(),
            cursor_blink: default_cursor_blink(),
            bell_notifications: default_bell_notifications(),
            restore_sessions: default_restore_sessions(),
            editor_command: String::new(),
        }
    }
}

pub fn path() -> Option<PathBuf> {
    let base = dirs::data_dir()?;
    Some(base.join(APP_DIR).join(FILENAME))
}

pub fn load() -> Preferences {
    let Some(p) = path() else { return Preferences::default() };
    match std::fs::read_to_string(&p) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => Preferences::default(),
    }
}

pub fn save(prefs: &Preferences) -> std::io::Result<()> {
    let Some(p) = path() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "data_dir unavailable",
        ));
    };
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(prefs)?;
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, &bytes)?;
    std::fs::rename(&tmp, &p)?;
    Ok(())
}

/// Validates and normalizes fields. Enforces sane bounds for font size, etc.
pub fn sanitize(mut p: Preferences) -> Preferences {
    if p.font_family.trim().is_empty() {
        p.font_family = default_font_family();
    }
    if p.font_size < 8 {
        p.font_size = 8;
    } else if p.font_size > 32 {
        p.font_size = 32;
    }
    if p.line_height < 1.0 {
        p.line_height = 1.0;
    } else if p.line_height > 2.0 {
        p.line_height = 2.0;
    }
    p
}
