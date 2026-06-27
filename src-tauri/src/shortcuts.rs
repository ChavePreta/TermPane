use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const FILENAME: &str = "shortcuts.json";
const APP_DIR: &str = "TermPane";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomShortcut {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub shell: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Shortcuts {
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default)]
    pub items: Vec<CustomShortcut>,
}

pub fn path() -> Option<PathBuf> {
    let base = dirs::data_dir()?;
    Some(base.join(APP_DIR).join(FILENAME))
}

pub fn load() -> Shortcuts {
    let Some(p) = path() else { return Shortcuts::default() };
    match std::fs::read_to_string(&p) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => Shortcuts::default(),
    }
}

pub fn save(shortcuts: &Shortcuts) -> std::io::Result<()> {
    let Some(p) = path() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "data_dir unavailable",
        ));
    };
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(shortcuts)?;
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, &bytes)?;
    std::fs::rename(&tmp, &p)?;
    Ok(())
}

/// Trims names, drops nameless entries, and collapses blank optional fields to None.
pub fn sanitize(mut s: Shortcuts) -> Shortcuts {
    s.items.retain_mut(|item| {
        item.name = item.name.trim().to_string();
        item.cwd = clean_opt(item.cwd.take());
        item.command = clean_opt(item.command.take());
        item.shell = clean_opt(item.shell.take());
        !item.name.is_empty()
    });
    s
}

fn clean_opt(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}
