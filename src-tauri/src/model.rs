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

impl SplitDir {
    pub fn flipped(self) -> Self {
        match self {
            Self::Horizontal => Self::Vertical,
            Self::Vertical => Self::Horizontal,
        }
    }
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

/// What a Claude Code session running in a pane is doing, as reported by its
/// hooks (`termpane hook`). `Ended` keeps the record (and its file list) after
/// Claude exits so the files can still be staged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClaudeStatus {
    Idle,
    Thinking,
    Tool,
    Permission,
    Waiting,
    Done,
    Ended,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeInfo {
    pub session_id: String,
    pub status: ClaudeStatus,
    pub detail: Option<String>,
    /// Claude's own working directory — differs from the shell's once Claude
    /// moves into a git worktree.
    pub cwd: Option<String>,
    /// Files this session wrote through Edit/Write tools, in first-touch order.
    pub files: Vec<String>,
    /// Commands and subagents Claude started in the background.
    #[serde(default)]
    pub background: Vec<BackgroundTask>,
    /// Artifacts (claude.ai pages) the session published.
    #[serde(default)]
    pub artifacts: Vec<ArtifactLink>,
    /// Fenced code blocks from Claude's messages, as written (newest last).
    #[serde(default)]
    pub code_blocks: Vec<CodeBlock>,
    /// Remote Control (`/remote-control`) is on for the session.
    #[serde(default)]
    pub remote: bool,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundTask {
    pub id: String,
    /// `bash` (a backgrounded command) or `agent` (a background subagent).
    pub kind: String,
    pub description: String,
    /// Where Claude writes the task's output, when it reported it; otherwise
    /// found under Claude's temp dir by task id.
    pub output_file: Option<String>,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeBlock {
    /// Stable for the same content, so the UI can tell new blocks apart.
    pub id: String,
    pub lang: String,
    pub code: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactLink {
    pub url: String,
    pub title: String,
}

/// Git context of a pane's working directory (Claude's while it runs there).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitInfo {
    pub repo: String,
    pub branch: String,
    /// Name of the linked worktree, when the directory is in one.
    pub worktree: Option<String>,
    /// The checkout's root (`--show-toplevel`); backend-only.
    #[serde(skip)]
    pub toplevel: String,
}

/// One listening server shown on a tab card.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortInfo {
    /// "127.0.0.1:5173".
    pub address: String,
    /// From the project's devcontainer.json `portsAttributes`.
    pub label: Option<String>,
    /// What clicking opens; None for databases, caches and non-local binds.
    pub url: Option<String>,
}

impl ClaudeInfo {
    pub fn is_live(&self) -> bool {
        self.status != ClaudeStatus::Ended
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaneSnapshot {
    pub id: String,
    pub foreground_command: Option<String>,
    pub exit_code: Option<i32>,
    pub claude: Option<ClaudeInfo>,
    pub git: Option<GitInfo>,
    /// Servers the tab's project listens on, for the card.
    pub ports: Vec<PortInfo>,
    /// Name of the project's dev container, when it has a devcontainer.json.
    pub dev_container: Option<String>,
    /// Working directory (Claude's while it runs there), refreshed with the
    /// rest of the slow context.
    pub cwd: Option<String>,
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
    pub parent_id: Option<String>,
    pub marked_unread: bool,
}
