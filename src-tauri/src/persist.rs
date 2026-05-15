use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const STATE_FILENAME: &str = "state.json";
const APP_DIR: &str = "TermPane";

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct PersistedState {
    pub schema_version: u32,
    pub next_label_index: u32,
}

pub fn state_path() -> Option<PathBuf> {
    let base = dirs::data_dir()?;
    Some(base.join(APP_DIR).join(STATE_FILENAME))
}

pub fn load() -> PersistedState {
    let Some(p) = state_path() else { return PersistedState::default() };
    match std::fs::read_to_string(&p) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => PersistedState::default(),
    }
}

pub fn save(state: &PersistedState) -> std::io::Result<()> {
    let Some(path) = state_path() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "data_dir unavailable",
        ));
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(state)?;
    // Atomic write: tempfile + rename in the same directory.
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &bytes)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}
