use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TerminalId(pub Uuid);

impl TerminalId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl std::fmt::Display for TerminalId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for TerminalId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(s).map(Self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PaneId(pub Uuid);

impl PaneId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl std::fmt::Display for PaneId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for PaneId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(s).map(Self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SplitDir {
    /// Children lado a lado (split vertical-line entre eles).
    Horizontal,
    /// Children empilhados verticalmente (split horizontal-line entre eles).
    Vertical,
}

/// Snapshot DTO da árvore de layout enviado ao frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum LayoutSnapshot {
    #[serde(rename = "leaf")]
    Leaf { id: String },
    #[serde(rename = "split")]
    Split {
        dir: SplitDir,
        children: Vec<LayoutSnapshot>,
        ratios: Vec<f32>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaneSnapshot {
    pub id: String,
    pub foreground_command: Option<String>,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSnapshot {
    pub id: String,
    pub label: String,
    pub created_at: DateTime<Utc>,
    pub layout: LayoutSnapshot,
    pub panes: Vec<PaneSnapshot>,
    pub active_pane: String,
}
