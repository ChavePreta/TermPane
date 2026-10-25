//! Claude Code integration.
//!
//! - Hook events (forwarded by `termpane hook` over the control socket) drive a
//!   per-pane `ClaudeInfo`: live status for the card, Claude's own cwd (which
//!   moves into git worktrees), and the files the session wrote.
//! - Past sessions are listed from Claude Code's own transcripts under
//!   `~/.claude/projects` (read-only) for the resume picker and restore.
//! - Staging only touches files no other live session also wrote.
//! - Everything Claude-side ships as a Claude Code plugin TermPane installs.

use crate::model::{
    ArtifactLink, BackgroundTask, ClaudeInfo, ClaudeStatus, CodeBlock, GitInfo, PaneId, TerminalId,
};
use crate::state::AppState;
use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::SystemTime;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ClaudeChanged {
    terminal_id: String,
    pane_id: String,
    claude: Option<ClaudeInfo>,
}

fn emit_change(app: &AppHandle, change: Option<(crate::model::TerminalId, Option<ClaudeInfo>)>, pane_id: PaneId) {
    if let Some((terminal_id, claude)) = change {
        let _ = app.emit(
            "pane:claude",
            ClaudeChanged {
                terminal_id: terminal_id.to_string(),
                pane_id: pane_id.to_string(),
                claude,
            },
        );
    }
}

const WRITE_TOOLS: &[&str] = &["Edit", "Write", "MultiEdit", "NotebookEdit"];

/// Applies one Claude Code hook payload (the JSON Claude passes on the hook's
/// stdin) to the pane it came from.
pub fn apply_hook(app: &AppHandle, pane_id: PaneId, payload: &Value) {
    let event = payload["hook_event_name"].as_str().unwrap_or_default().to_string();
    let Some(session_id) = payload["session_id"].as_str().map(str::to_string) else {
        return;
    };
    let cwd = payload["cwd"].as_str().map(str::to_string);
    if event == "Notification" && !permission_dialog_likely(payload, &session_id) {
        return;
    }
    let state = app.state::<AppState>();
    let change = state.update_claude(pane_id, |slot| {
        let same_session = slot.as_ref().is_some_and(|c| c.session_id == session_id);
        if !same_session {
            if event == "SessionEnd" {
                return;
            }
            *slot = Some(ClaudeInfo {
                session_id: session_id.clone(),
                status: ClaudeStatus::Idle,
                detail: None,
                cwd: None,
                files: Vec::new(),
                background: Vec::new(),
                artifacts: Vec::new(),
                code_blocks: Vec::new(),
                updated_at: Utc::now(),
            });
        }
        let info = slot.as_mut().expect("set above");
        if cwd.is_some() {
            info.cwd = cwd.clone();
        }
        info.updated_at = Utc::now();
        match event.as_str() {
            "SessionStart" => {
                info.status = ClaudeStatus::Idle;
                info.detail = None;
            }
            "UserPromptSubmit" => {
                info.status = ClaudeStatus::Thinking;
                info.detail = None;
            }
            "PreToolUse" => {
                info.status = ClaudeStatus::Tool;
                info.detail = Some(describe_tool(
                    payload["tool_name"].as_str().unwrap_or("tool"),
                    &payload["tool_input"],
                ));
            }
            "PostToolUse" => {
                info.status = ClaudeStatus::Thinking;
                info.detail = None;
                let tool = payload["tool_name"].as_str().unwrap_or_default();
                if WRITE_TOOLS.contains(&tool) {
                    let input = &payload["tool_input"];
                    let raw = input["file_path"].as_str().or(input["notebook_path"].as_str());
                    if let Some(raw) = raw {
                        let abs = absolutize(raw, info.cwd.as_deref());
                        if !info.files.contains(&abs) {
                            info.files.push(abs);
                        }
                    }
                }
            }
            "Notification" => {
                let message = payload["message"].as_str().unwrap_or_default();
                let kind = payload["notification_type"].as_str().unwrap_or_default();
                let permission = kind.contains("permission")
                    || message.to_ascii_lowercase().contains("permission");
                info.status = if permission {
                    ClaudeStatus::Permission
                } else {
                    ClaudeStatus::Waiting
                };
                info.detail = (!message.is_empty()).then(|| message.to_string());
            }
            "Stop" => {
                info.status = ClaudeStatus::Done;
                info.detail = None;
            }
            "SessionEnd" => {
                info.status = ClaudeStatus::Ended;
                info.detail = None;
            }
            // Sent by TermPane's own plugin: a command or subagent Claude
            // started in the background.
            "TermPaneBackground" => {
                let t = &payload["task"];
                if let Some(id) = t["id"].as_str().filter(|s| !s.is_empty()) {
                    if !info.background.iter().any(|b| b.id == id) {
                        info.background.push(BackgroundTask {
                            id: id.to_string(),
                            kind: t["kind"].as_str().unwrap_or("bash").to_string(),
                            description: t["description"].as_str().unwrap_or_default().to_string(),
                            output_file: t["outputFile"].as_str().map(str::to_string),
                            started_at: Utc::now(),
                        });
                    }
                }
            }
            // Sent by TermPane's own plugin: an artifact the session published.
            "TermPaneArtifact" => {
                let a = &payload["artifact"];
                if let Some(url) = a["url"].as_str().filter(|u| u.starts_with("https://")) {
                    let title = a["title"].as_str().filter(|t| !t.is_empty()).unwrap_or(url);
                    match info.artifacts.iter_mut().find(|x| x.url == url) {
                        Some(existing) => existing.title = title.to_string(),
                        None => info.artifacts.push(ArtifactLink {
                            url: url.to_string(),
                            title: title.to_string(),
                        }),
                    }
                }
            }
            _ => {}
        }
    });
    emit_change(app, change, pane_id);
    // The session title shows up after the first turn and changes on /rename;
    // check it whenever a turn ends or a session starts.
    if matches!(event.as_str(), "Stop" | "SessionStart") {
        let app = app.clone();
        std::thread::spawn(move || {
            follow_session_title(&app, pane_id, &session_id);
            collect_code_blocks(&app, pane_id, &session_id);
            // The turn's last message can reach the transcript just after Stop.
            std::thread::sleep(std::time::Duration::from_millis(1500));
            collect_code_blocks(&app, pane_id, &session_id);
        });
    }
}

// --- Code blocks -------------------------------------------------------------

// How far each session's transcript has been scanned for code blocks.
fn scanned_upto() -> &'static Mutex<HashMap<String, u64>> {
    static SCANNED: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();
    SCANNED.get_or_init(|| Mutex::new(HashMap::new()))
}

const MAX_CODE_BLOCKS: usize = 60;
// A resumed or first-seen session is scanned from this far back only.
const CODE_SCAN_BACK: u64 = 1024 * 1024;

/// Reads Claude's messages added to the transcript since the last scan and
/// records their fenced code blocks on the pane, verbatim — the copy buttons
/// copy this, not the screen, so indentation and blank lines survive.
fn collect_code_blocks(app: &AppHandle, pane_id: PaneId, session_id: &str) {
    let Some(path) = transcript_path(session_id) else { return };
    let Ok(mut file) = std::fs::File::open(&path) else { return };
    let Ok(len) = file.metadata().map(|m| m.len()) else { return };
    let start = {
        let map = scanned_upto().lock();
        map.get(session_id).copied().unwrap_or(len.saturating_sub(CODE_SCAN_BACK))
    };
    if start >= len || file.seek(SeekFrom::Start(start)).is_err() {
        return;
    }
    let mut bytes = Vec::new();
    if file.read_to_end(&mut bytes).is_err() {
        return;
    }
    // Only up to the last complete line; a half-written one is read next time.
    let Some(end) = bytes.iter().rposition(|b| *b == b'\n') else { return };
    scanned_upto().lock().insert(session_id.to_string(), start + end as u64 + 1);
    let mut found: Vec<CodeBlock> = Vec::new();
    for line in bytes[..end].split(|b| *b == b'\n') {
        let Ok(v) = serde_json::from_slice::<Value>(line) else { continue };
        if v["type"] != "assistant" || v["isSidechain"] == true {
            continue;
        }
        for part in v["message"]["content"].as_array().into_iter().flatten() {
            if let Some(text) = part["text"].as_str().filter(|_| part["type"] == "text") {
                found.extend(fenced_blocks(text));
            }
        }
    }
    if found.is_empty() {
        return;
    }
    let state = app.state::<AppState>();
    let change = state.update_claude(pane_id, |slot| {
        let Some(info) = slot.as_mut() else { return };
        for block in found {
            info.code_blocks.retain(|b| b.id != block.id);
            info.code_blocks.push(block);
        }
        let excess = info.code_blocks.len().saturating_sub(MAX_CODE_BLOCKS);
        info.code_blocks.drain(..excess);
    });
    emit_change(app, change, pane_id);
}

/// Markdown fenced code blocks (``` or ~~~, optional language) in `text`. A
/// fence indented inside a list has that indent removed from its lines.
pub fn fenced_blocks(text: &str) -> Vec<CodeBlock> {
    use std::hash::{Hash, Hasher};
    let mut out = Vec::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        let fence = if trimmed.starts_with("```") {
            "```"
        } else if trimmed.starts_with("~~~") {
            "~~~"
        } else {
            continue;
        };
        let indent = line.len() - trimmed.len();
        let lang = trimmed[3..].trim().to_string();
        let mut body: Vec<&str> = Vec::new();
        let mut closed = false;
        for inner in lines.by_ref() {
            if inner.trim_start().starts_with(fence) && inner.trim_start()[3..].trim().is_empty() {
                closed = true;
                break;
            }
            let strip = inner.len() - inner.trim_start().len();
            body.push(&inner[strip.min(indent)..]);
        }
        let code = body.join("\n");
        if !closed || code.trim().is_empty() {
            continue;
        }
        let mut h = std::collections::hash_map::DefaultHasher::new();
        code.hash(&mut h);
        out.push(CodeBlock {
            id: format!("{:016x}", h.finish()),
            lang,
            code,
        });
    }
    out
}

// --- Tab names <-> session names ---------------------------------------------

/// Claude Code keeps each session as `~/.claude/projects/<dir>/<id>.jsonl`;
/// the directory is derived from the start cwd, so look the id up instead.
pub fn transcript_path(session_id: &str) -> Option<PathBuf> {
    let root = dirs::home_dir()?.join(".claude").join("projects");
    let file = format!("{session_id}.jsonl");
    std::fs::read_dir(root)
        .ok()?
        .flatten()
        .map(|d| d.path().join(&file))
        .find(|p| p.is_file())
}

/// The session's current title: its custom title (`/rename`, `-n`, or one
/// TermPane wrote), else the one Claude generated.
fn session_title(session_id: &str) -> Option<String> {
    let path = transcript_path(session_id)?;
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    file.seek(SeekFrom::Start(len.saturating_sub(TAIL_BYTES))).ok()?;
    let mut tail = Vec::new();
    file.take(TAIL_BYTES).read_to_end(&mut tail).ok()?;
    let (mut custom, mut ai) = (None, None);
    for line in tail.split(|b| *b == b'\n') {
        let Ok(v) = serde_json::from_slice::<Value>(line) else { continue };
        match v["type"].as_str() {
            Some("custom-title") => custom = v["customTitle"].as_str().map(str::to_string),
            Some("ai-title") => ai = v["aiTitle"].as_str().map(str::to_string),
            _ => {}
        }
    }
    custom.or(ai).filter(|t| !t.trim().is_empty())
}

/// Short form of a session title for a tab label: whole words up to ~28
/// characters, "…" when cut.
fn short_label(title: &str) -> String {
    const MAX: usize = 28;
    let title = title.trim();
    if title.chars().count() <= MAX {
        return title.to_string();
    }
    let mut out = String::new();
    for word in title.split_whitespace() {
        if out.chars().count() + word.chars().count() + 1 > MAX - 1 {
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    if out.is_empty() {
        out = title.chars().take(MAX - 1).collect();
    }
    out.push('…');
    out
}

/// Renames an auto-labelled tab after its Claude session's title.
fn follow_session_title(app: &AppHandle, pane_id: PaneId, session_id: &str) {
    let state = app.state::<AppState>();
    let Some(terminal_id) = state.terminal_of_pane(pane_id) else { return };
    if !state.is_auto_label(terminal_id) {
        return;
    }
    let Some(title) = session_title(session_id) else { return };
    let label = short_label(&title);
    let current = state.terminal_snapshot(terminal_id).map(|s| s.label);
    if current.as_deref() == Some(label.as_str()) {
        return;
    }
    if state.rename_terminal(terminal_id, label, false) {
        if let Some(s) = state.terminal_snapshot(terminal_id) {
            let _ = app.emit("terminal:updated", s);
        }
    }
}

/// Gives every live Claude session in the tab a name derived from the tab's
/// label: the label plus where it runs ("Login bug — zumpost-dag/feat-x"), so
/// the /resume picker tells sessions of different projects apart. Written as
/// the `custom-title` entry `/rename` itself appends to the transcript; a
/// running session shows the new name in its prompt box from its next start.
pub fn name_sessions_after_tab(state: &AppState, terminal_id: TerminalId, label: &str) {
    let Some(snap) = state.terminal_snapshot(terminal_id) else { return };
    for pane in &snap.panes {
        let Some(c) = pane.claude.as_ref().filter(|c| c.is_live()) else { continue };
        let long = match &pane.git {
            Some(GitInfo { repo, worktree: Some(wt), .. }) => format!("{label} — {repo} (worktree {wt})"),
            Some(GitInfo { repo, branch, .. }) => format!("{label} — {repo}/{branch}"),
            None => label.to_string(),
        };
        if let Some(path) = transcript_path(&c.session_id) {
            let entry = serde_json::json!({
                "type": "custom-title",
                "customTitle": long,
                "sessionId": c.session_id,
            });
            use std::io::Write;
            match std::fs::OpenOptions::new().append(true).open(&path) {
                Ok(mut f) => {
                    if let Err(e) = writeln!(f, "{entry}") {
                        tracing::warn!("naming session {}: {e}", c.session_id);
                    }
                }
                Err(e) => tracing::warn!("naming session {}: {e}", c.session_id),
            }
        }
    }
}

/// Whether a permission notification from TermPane's plugin (sent when the
/// permission check says "ask") will really put a dialog in front of the user.
/// In auto mode Claude's classifier settles most asks without one — only those
/// forced by an ask rule, a hook or a setting still prompt — and dontAsk never
/// prompts. Other notifications pass through.
fn permission_dialog_likely(payload: &Value, session_id: &str) -> bool {
    if payload["notification_type"] != "permission_prompt" {
        return true;
    }
    let forced = payload["forced"].as_bool().unwrap_or(false);
    match session_permission_mode(session_id).as_deref() {
        Some("dontAsk") => false,
        Some("auto") | Some("bypassPermissions") => forced,
        _ => true,
    }
}

/// The session's current permission mode, from the latest `permission-mode`
/// entry in its transcript (Claude writes one per prompt and on each change).
fn session_permission_mode(session_id: &str) -> Option<String> {
    const SCAN: u64 = 512 * 1024;
    let path = transcript_path(session_id)?;
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    file.seek(SeekFrom::Start(len.saturating_sub(SCAN))).ok()?;
    let mut tail = Vec::new();
    file.take(SCAN).read_to_end(&mut tail).ok()?;
    tail.split(|b| *b == b'\n').rev().find_map(|line| {
        if !line.windows(17).any(|w| w == b"\"permission-mode\"") {
            return None;
        }
        let v: Value = serde_json::from_slice(line).ok()?;
        (v["type"] == "permission-mode")
            .then(|| v["permissionMode"].as_str().map(str::to_string))
            .flatten()
    })
}

/// Called by the monitor when a pane's shell is back at its prompt: whatever
/// Claude session ran there is gone, even if it exited without `SessionEnd`.
pub fn mark_ended(app: &AppHandle, pane_id: PaneId) {
    let state = app.state::<AppState>();
    let change = state.update_claude(pane_id, |slot| {
        if let Some(info) = slot.as_mut() {
            if info.is_live() {
                info.status = ClaudeStatus::Ended;
                info.detail = None;
            }
        }
    });
    emit_change(app, change, pane_id);
}

pub fn clear_files(app: &AppHandle, pane_id: PaneId) {
    let state = app.state::<AppState>();
    let change = state.update_claude(pane_id, |slot| {
        if let Some(info) = slot.as_mut() {
            info.files.clear();
        }
    });
    emit_change(app, change, pane_id);
}

fn absolutize(path: &str, cwd: Option<&str>) -> String {
    let p = Path::new(path);
    if p.is_absolute() {
        return path.to_string();
    }
    match cwd {
        Some(c) => Path::new(c).join(p).to_string_lossy().into_owned(),
        None => path.to_string(),
    }
}

fn describe_tool(tool: &str, input: &Value) -> String {
    let base = |key: &str| {
        input[key]
            .as_str()
            .and_then(|p| Path::new(p).file_name())
            .map(|f| f.to_string_lossy().into_owned())
    };
    let text = match tool {
        "Bash" => input["command"]
            .as_str()
            .map(|c| format!("Bash: {}", c.lines().next().unwrap_or_default())),
        "Edit" | "MultiEdit" | "Write" | "Read" => {
            base("file_path").map(|f| format!("{tool} {f}"))
        }
        "NotebookEdit" => base("notebook_path").map(|f| format!("{tool} {f}")),
        "Grep" | "Glob" => input["pattern"].as_str().map(|p| format!("{tool} {p}")),
        "Task" | "Agent" => input["description"].as_str().map(|d| format!("Agent: {d}")),
        "WebFetch" => input["url"].as_str().map(|u| format!("WebFetch {u}")),
        "WebSearch" => input["query"].as_str().map(|q| format!("WebSearch {q}")),
        _ => None,
    };
    let text = text.unwrap_or_else(|| match tool.strip_prefix("mcp__") {
        Some(rest) => format!("MCP {}", rest.rsplit("__").next().unwrap_or(rest)),
        None => tool.to_string(),
    });
    truncate(&text, 70)
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max - 1).collect();
        out.push('…');
        out
    }
}

// --- Staging ---------------------------------------------------------------

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StageResult {
    pub staged: Vec<String>,
    /// Also written by another session in this window — left for the user.
    pub shared: Vec<String>,
    pub errors: Vec<String>,
}

/// Files of `pane_id`'s session that another session in this window also wrote.
pub fn shared_files(state: &AppState, pane_id: PaneId) -> Vec<String> {
    let all = state.all_claude();
    let Some((_, mine)) = all.iter().find(|(p, _)| *p == pane_id) else {
        return Vec::new();
    };
    mine.files
        .iter()
        .filter(|f| {
            all.iter()
                .any(|(p, c)| *p != pane_id && c.session_id != mine.session_id && c.files.contains(f))
        })
        .cloned()
        .collect()
}

pub fn stage_files(state: &AppState, pane_id: PaneId) -> StageResult {
    let mut result = StageResult::default();
    let Some(info) = state.claude_of(pane_id) else {
        return result;
    };
    result.shared = shared_files(state, pane_id);
    let mut by_repo: HashMap<PathBuf, Vec<String>> = HashMap::new();
    for file in info.files.iter().filter(|f| !result.shared.contains(f)) {
        let dir = Path::new(file)
            .parent()
            .filter(|d| d.is_dir())
            .map(Path::to_path_buf)
            .or_else(|| info.cwd.as_ref().map(PathBuf::from));
        let Some(dir) = dir else {
            result.errors.push(format!("{file}: no directory to resolve the repository"));
            continue;
        };
        match git_toplevel(&dir) {
            Some(top) => by_repo.entry(top).or_default().push(file.clone()),
            None => result.errors.push(format!("{file}: not inside a git repository")),
        }
    }
    for (top, files) in by_repo {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(&top)
            .args(["add", "-A", "--"])
            .args(&files)
            .output();
        match out {
            Ok(o) if o.status.success() => result.staged.extend(files),
            Ok(o) => result.errors.push(format!(
                "{}: {}",
                top.display(),
                String::from_utf8_lossy(&o.stderr).trim()
            )),
            Err(e) => result.errors.push(format!("git: {e}")),
        }
    }
    result
}

fn git_toplevel(dir: &Path) -> Option<PathBuf> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then(|| PathBuf::from(s))
}

// --- Past sessions ---------------------------------------------------------

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeSession {
    pub id: String,
    pub title: String,
    /// Where the session was started — `claude --resume` must run from here.
    pub cwd: String,
    pub git_branch: Option<String>,
    pub started_at: Option<String>,
    pub modified_at: DateTime<Utc>,
}

const MAX_SESSIONS: usize = 300;
const HEAD_BYTES: u64 = 128 * 1024;
const TAIL_BYTES: u64 = 256 * 1024;

type SessionCache = HashMap<PathBuf, (SystemTime, u64, Option<ClaudeSession>)>;

fn session_cache() -> &'static Mutex<SessionCache> {
    static CACHE: OnceLock<Mutex<SessionCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Most recent Claude Code sessions, newest first. Transcripts can be huge, so
/// only the head (first prompt, start cwd) and the tail (latest title, branch)
/// of each file are read, and results are cached by mtime + size.
pub fn list_sessions() -> Vec<ClaudeSession> {
    let Some(root) = dirs::home_dir().map(|h| h.join(".claude").join("projects")) else {
        return Vec::new();
    };
    let mut files: Vec<(PathBuf, SystemTime, u64)> = Vec::new();
    let Ok(projects) = std::fs::read_dir(&root) else {
        return Vec::new();
    };
    for project in projects.flatten() {
        let Ok(entries) = std::fs::read_dir(project.path()) else { continue };
        for e in entries.flatten() {
            let path = e.path();
            if path.extension().is_none_or(|x| x != "jsonl") {
                continue;
            }
            if let Ok(meta) = e.metadata() {
                files.push((path, meta.modified().unwrap_or(SystemTime::UNIX_EPOCH), meta.len()));
            }
        }
    }
    files.sort_by(|a, b| b.1.cmp(&a.1));
    files.truncate(MAX_SESSIONS);

    let mut cache = session_cache().lock();
    let mut out = Vec::new();
    for (path, mtime, size) in files {
        let cached = cache
            .get(&path)
            .filter(|(m, s, _)| *m == mtime && *s == size)
            .map(|(_, _, v)| v.clone());
        let session = match cached {
            Some(v) => v,
            None => {
                let v = read_session(&path, mtime);
                cache.insert(path.clone(), (mtime, size, v.clone()));
                v
            }
        };
        if let Some(s) = session {
            out.push(s);
        }
    }
    out
}

fn read_session(path: &Path, mtime: SystemTime) -> Option<ClaudeSession> {
    let id = path.file_stem()?.to_string_lossy().into_owned();
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();

    let mut head = Vec::new();
    (&mut file).take(HEAD_BYTES).read_to_end(&mut head).ok()?;
    let mut cwd: Option<String> = None;
    let mut started_at: Option<String> = None;
    let mut first_prompt: Option<String> = None;
    let mut branch: Option<String> = None;
    for line in head.split(|b| *b == b'\n') {
        let Ok(v) = serde_json::from_slice::<Value>(line) else { continue };
        if cwd.is_none() {
            cwd = v["cwd"].as_str().map(str::to_string);
        }
        if branch.is_none() {
            branch = v["gitBranch"].as_str().filter(|b| !b.is_empty()).map(str::to_string);
        }
        if first_prompt.is_none() && v["type"] == "user" && v["isSidechain"] != true && v["isMeta"] != true {
            if let Some(text) = prompt_text(&v["message"]["content"]) {
                first_prompt = Some(text);
                started_at = v["timestamp"].as_str().map(str::to_string);
            }
        }
        if cwd.is_some() && first_prompt.is_some() {
            break;
        }
    }

    let mut title: Option<String> = None;
    let mut ai_title: Option<String> = None;
    let mut tail = Vec::new();
    if len > 0 {
        let start = len.saturating_sub(TAIL_BYTES);
        file.seek(SeekFrom::Start(start)).ok()?;
        file.take(TAIL_BYTES).read_to_end(&mut tail).ok()?;
    }
    for line in tail.split(|b| *b == b'\n') {
        let Ok(v) = serde_json::from_slice::<Value>(line) else { continue };
        match v["type"].as_str() {
            Some("custom-title") => title = v["customTitle"].as_str().map(str::to_string),
            Some("ai-title") => ai_title = v["aiTitle"].as_str().map(str::to_string),
            _ => {}
        }
        if let Some(b) = v["gitBranch"].as_str().filter(|b| !b.is_empty()) {
            branch = Some(b.to_string());
        }
    }

    let cwd = cwd?;
    let title = title.or(ai_title).or(first_prompt)?;
    Some(ClaudeSession {
        id,
        title: truncate(title.lines().next().unwrap_or_default().trim(), 90),
        cwd,
        git_branch: branch,
        started_at,
        modified_at: DateTime::<Utc>::from(mtime),
    })
}

/// A human-typed prompt: skips slash-command echoes, system reminders and
/// tool results, which all arrive as user-role content starting with a tag.
fn prompt_text(content: &Value) -> Option<String> {
    let text = match content {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .find(|p| p["type"] == "text")
            .and_then(|p| p["text"].as_str())?
            .to_string(),
        _ => return None,
    };
    let t = text.trim();
    (!t.is_empty() && !t.starts_with('<')).then(|| t.to_string())
}

// --- Claude Code plugin installer -------------------------------------------
//
// Everything Claude-side (status forwarding, tab tools, /fork) ships as one
// Claude Code plugin, `integrations/claude-code/plugin/` in the repo, embedded
// here. Installing writes it to `<data dir>/TermPane/claude-plugin/` (a local
// marketplace folder) and runs `claude plugin marketplace add` + `install`.
// Claude reads a folder marketplace's plugin straight from that folder, so a
// TermPane update that rewrites the files reaches sessions on /reload-plugins
// with no reinstall.

const PLUGIN_FILES: &[(&str, &str)] = &[
    (
        ".claude-plugin/marketplace.json",
        include_str!("../../integrations/claude-code/plugin/.claude-plugin/marketplace.json"),
    ),
    (
        "termpane/.claude-plugin/plugin.json",
        include_str!("../../integrations/claude-code/plugin/termpane/.claude-plugin/plugin.json"),
    ),
    (
        "termpane/hooks/hooks.json",
        include_str!("../../integrations/claude-code/plugin/termpane/hooks/hooks.json"),
    ),
    (
        "termpane/hooks/register.ts",
        include_str!("../../integrations/claude-code/plugin/termpane/hooks/register.ts"),
    ),
];

const PLUGIN_ID: &str = "termpane@termpane";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationStatus {
    pub plugin_installed: bool,
    /// Hooks or an MCP server left by the earlier settings-based integration;
    /// the plugin installer removes them.
    pub legacy_setup: bool,
    /// The commands to run by hand when the installer can't reach `claude`.
    pub manual_commands: String,
}

pub fn exe_path() -> String {
    std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "termpane".to_string())
}

fn plugin_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("TermPane").join("claude-plugin"))
}

fn settings_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude").join("settings.json"))
}

fn is_termpane_hook(cmd: &str) -> bool {
    cmd.to_ascii_lowercase().contains("termpane") && cmd.trim_end().ends_with(" hook")
}

fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn manual_commands() -> String {
    let dir = plugin_dir().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default();
    format!(
        "claude plugin marketplace add {} && claude plugin install {PLUGIN_ID}",
        quote(&dir)
    )
}

/// Runs a command line in the user's login shell — GUI apps don't inherit the
/// PATH where `claude` lives.
fn login_shell(cmd: &str) -> Result<String, String> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let out = std::process::Command::new(shell)
        .args(["-lc", cmd])
        .output()
        .map_err(|e| format!("could not run the shell: {e}"))?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
    .trim()
    .to_string();
    if out.status.success() {
        Ok(text)
    } else {
        Err(text)
    }
}

fn legacy_hooks_present() -> bool {
    settings_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|v| v["hooks"].as_object().cloned())
        .is_some_and(|hooks| {
            hooks.values().any(|groups| {
                groups.as_array().is_some_and(|gs| {
                    gs.iter().any(|g| {
                        g["hooks"].as_array().is_some_and(|hs| {
                            hs.iter().any(|h| h["command"].as_str().is_some_and(is_termpane_hook))
                        })
                    })
                })
            })
        })
}

fn legacy_mcp_present() -> bool {
    dirs::home_dir()
        .and_then(|h| std::fs::read_to_string(h.join(".claude.json")).ok())
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .is_some_and(|v| v["mcpServers"]["termpane"].is_object())
}

pub fn integration_status() -> IntegrationStatus {
    let plugin_installed = login_shell("claude plugin list --json")
        .map(|out| out.contains(PLUGIN_ID))
        .unwrap_or(false);
    IntegrationStatus {
        plugin_installed,
        legacy_setup: legacy_hooks_present() || legacy_mcp_present(),
        manual_commands: manual_commands(),
    }
}

/// Rewrites the plugin files from this build when the plugin was installed
/// before, so a TermPane update carries its plugin along: Claude reads a folder
/// marketplace's plugin straight from disk, and sessions started afterwards
/// (or after /reload-plugins) run the new version.
pub fn refresh_installed_plugin() {
    let Some(dir) = plugin_dir() else { return };
    if !dir.join(".claude-plugin").is_dir() {
        return;
    }
    for (rel, content) in PLUGIN_FILES {
        let path = dir.join(rel);
        if std::fs::read_to_string(&path).is_ok_and(|current| current == *content) {
            continue;
        }
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&path, content) {
            tracing::warn!("refreshing plugin file {}: {e}", path.display());
        }
    }
}

/// Writes the embedded plugin to disk, installs it with Claude Code, and
/// removes the earlier settings-based setup (TermPane's own hook entries in
/// `~/.claude/settings.json` and the `termpane` MCP server) so nothing reports
/// twice. Returns what `claude` printed.
pub fn install_plugin() -> Result<String, String> {
    let dir = plugin_dir().ok_or("data directory unavailable")?;
    for (rel, content) in PLUGIN_FILES {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, content).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    let dir_arg = quote(&dir.to_string_lossy());
    // `marketplace add` fails harmlessly when it is already known.
    let out = login_shell(&format!(
        "claude plugin marketplace add {dir_arg} >/dev/null 2>&1; claude plugin install {PLUGIN_ID}"
    ))?;
    remove_legacy_hooks()?;
    if legacy_mcp_present() {
        let _ = login_shell("claude mcp remove --scope user termpane");
    }
    Ok(out)
}

/// Drops TermPane's hook entries from `~/.claude/settings.json`, keeping every
/// other hook and setting. Backup + atomic write, as the file is the user's.
fn remove_legacy_hooks() -> Result<(), String> {
    if !legacy_hooks_present() {
        return Ok(());
    }
    let path = settings_path().ok_or("home directory unavailable")?;
    let raw = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut root: Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    if let Some(hooks) = root["hooks"].as_object_mut() {
        for groups in hooks.values_mut() {
            if let Some(gs) = groups.as_array_mut() {
                for g in gs.iter_mut() {
                    if let Some(hs) = g["hooks"].as_array_mut() {
                        hs.retain(|h| !h["command"].as_str().is_some_and(is_termpane_hook));
                    }
                }
                gs.retain(|g| g["hooks"].as_array().is_none_or(|hs| !hs.is_empty()));
            }
        }
        hooks.retain(|_, groups| groups.as_array().is_none_or(|gs| !gs.is_empty()));
    }
    std::fs::copy(&path, path.with_extension("json.termpane-backup"))
        .map_err(|e| format!("backup failed: {e}"))?;
    let bytes = serde_json::to_vec_pretty(&root).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.termpane-tmp");
    std::fs::write(&tmp, bytes).map_err(|e| format!("write failed: {e}"))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("write failed: {e}"))?;
    Ok(())
}

// --- Background tasks --------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundOutput {
    pub text: String,
    pub path: Option<String>,
    /// The output file changed in the last few seconds.
    pub active: bool,
}

const OUTPUT_TAIL: u64 = 256 * 1024;

/// Claude keeps a task's output in `<temp>/claude-<uid>/<project>/<session>/
/// tasks/<id>.output`; the temp root differs by platform.
fn find_task_output(session_id: &str, task_id: &str) -> Option<PathBuf> {
    let uid = nix::unistd::Uid::current();
    let mut roots = vec![
        PathBuf::from(format!("/private/tmp/claude-{uid}")),
        PathBuf::from(format!("/tmp/claude-{uid}")),
    ];
    if let Ok(t) = std::env::var("TMPDIR") {
        roots.push(Path::new(&t).join(format!("claude-{uid}")));
    }
    let file = format!("{task_id}.output");
    roots.iter().find_map(|root| {
        std::fs::read_dir(root)
            .ok()?
            .flatten()
            .map(|p| p.path().join(session_id).join("tasks").join(&file))
            .find(|p| p.is_file())
    })
}

pub fn read_background(state: &AppState, pane_id: PaneId, task_id: &str) -> Result<BackgroundOutput, String> {
    let info = state.claude_of(pane_id).ok_or("no Claude session in this pane")?;
    let task = info.background.iter().find(|t| t.id == task_id).ok_or("task not found")?;
    let path = task
        .output_file
        .as_ref()
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .or_else(|| find_task_output(&info.session_id, &task.id));
    let Some(path) = path else {
        return Ok(BackgroundOutput {
            text: String::new(),
            path: None,
            active: false,
        });
    };
    let mut file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
    let meta = file.metadata().map_err(|e| e.to_string())?;
    let active = meta
        .modified()
        .ok()
        .and_then(|m| m.elapsed().ok())
        .is_some_and(|age| age.as_secs() < 10);
    let len = meta.len();
    file.seek(SeekFrom::Start(len.saturating_sub(OUTPUT_TAIL))).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take(OUTPUT_TAIL).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    let text = if task.kind == "agent" {
        render_agent_transcript(&bytes)
    } else {
        crate::control::readable_tail(&bytes, 160, 1000)
    };
    Ok(BackgroundOutput {
        text,
        path: Some(path.to_string_lossy().into_owned()),
        active,
    })
}

/// A subagent's output file is its JSONL transcript; show what a person would
/// follow: prompts, Claude's text, tool calls and the head of their results.
fn render_agent_transcript(bytes: &[u8]) -> String {
    let mut out: Vec<String> = Vec::new();
    for line in bytes.split(|b| *b == b'\n') {
        let Ok(v) = serde_json::from_slice::<Value>(line) else { continue };
        let content = &v["message"]["content"];
        match v["type"].as_str() {
            Some("user") => {
                if let Some(s) = content.as_str() {
                    out.push(format!("▶ {}", s.trim()));
                }
                for part in content.as_array().into_iter().flatten() {
                    if part["type"] == "tool_result" {
                        let text = match &part["content"] {
                            Value::String(s) => s.clone(),
                            Value::Array(a) => a
                                .iter()
                                .filter_map(|x| x["text"].as_str())
                                .collect::<Vec<_>>()
                                .join("\n"),
                            _ => String::new(),
                        };
                        let head: Vec<&str> = text.lines().take(3).collect();
                        if !head.is_empty() {
                            out.push(format!("  ⎿ {}", truncate(&head.join("\n    "), 400)));
                        }
                    } else if let Some(t) = part["text"].as_str() {
                        if !t.trim_start().starts_with('<') {
                            out.push(format!("▶ {}", t.trim()));
                        }
                    }
                }
            }
            Some("assistant") => {
                for part in content.as_array().into_iter().flatten() {
                    match part["type"].as_str() {
                        Some("text") => {
                            if let Some(t) = part["text"].as_str().filter(|t| !t.trim().is_empty()) {
                                out.push(t.trim().to_string());
                            }
                        }
                        Some("tool_use") => out.push(format!(
                            "→ {}",
                            describe_tool(part["name"].as_str().unwrap_or("tool"), &part["input"])
                        )),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    out.join("\n\n")
}

// --- Diff and opening files ----------------------------------------------------

/// What changed in `path` against HEAD (staged and unstaged together); a new,
/// untracked file shows as fully added.
pub fn file_diff(path: &str) -> Result<String, String> {
    let p = Path::new(path);
    let dir = p.parent().filter(|d| d.is_dir()).ok_or("the file's folder is gone")?;
    let top = git_toplevel(dir).ok_or("not inside a git repository")?;
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(&top)
            .args(args)
            .output()
            .map_err(|e| format!("git: {e}"))
    };
    let tracked = git(&["ls-files", "--error-unmatch", "--", path])?.status.success();
    let out = if tracked {
        git(&["diff", "--no-color", "HEAD", "--", path])?
    } else if p.is_file() {
        git(&["diff", "--no-color", "--no-index", "--", "/dev/null", path])?
    } else {
        return Ok(String::new());
    };
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn quote_arg(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Opens a file (at a line, when known) with the editor command from
/// preferences (`{file}`, `{line}`, `{col}` placeholders), falling back to
/// `code -g`, then to the system's default app.
pub fn open_in_editor(path: &str, line: Option<u32>, col: Option<u32>, cwd: Option<&str>) -> Result<(), String> {
    let mut p = PathBuf::from(path);
    if p.is_relative() {
        if let Some(c) = cwd {
            p = Path::new(c).join(p);
        }
    }
    if !p.exists() {
        return Err(format!("{} does not exist", p.display()));
    }
    let file = p.to_string_lossy().into_owned();
    let configured = crate::preferences::load().editor_command;
    let template = if configured.trim().is_empty() {
        "code -g {file}:{line}:{col}".to_string()
    } else {
        configured
    };
    let cmd = template
        .replace("{file}", &quote_arg(&file))
        .replace("{line}", &line.unwrap_or(1).to_string())
        .replace("{col}", &col.unwrap_or(1).to_string());
    if login_shell(&cmd).is_ok() {
        return Ok(());
    }
    let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    std::process::Command::new(opener)
        .arg(&file)
        .status()
        .map_err(|e| e.to_string())
        .and_then(|s| if s.success() { Ok(()) } else { Err(format!("{opener} failed")) })
}

#[cfg(test)]
mod tests {
    use super::{fenced_blocks, render_agent_transcript, short_label};

    #[test]
    fn fenced_blocks_keep_the_code_verbatim() {
        let text = "Run this:\n\n```bash\naws ec2 run-instances \\\n  --key-name zum-prod\n```\n\n1. Then:\n   ```\n   if x:\n       y()\n   ```\nUnclosed:\n```\nnope";
        let blocks = fenced_blocks(text);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].lang, "bash");
        assert_eq!(blocks[0].code, "aws ec2 run-instances \\\n  --key-name zum-prod");
        assert_eq!(blocks[1].code, "if x:\n    y()");
    }

    #[test]
    fn short_label_keeps_whole_words() {
        assert_eq!(short_label("Bugs do termpane"), "Bugs do termpane");
        assert_eq!(
            short_label("Fix the login redirect loop after OAuth callback"),
            "Fix the login redirect loop…"
        );
        assert!(short_label("Supercalifragilisticexpialidocious-and-more").ends_with('…'));
    }

    #[test]
    fn agent_transcript_reads_like_a_conversation() {
        let jsonl = br#"{"type":"user","message":{"content":"Find the config loader"}}
{"type":"assistant","message":{"content":[{"type":"text","text":"Looking."},{"type":"tool_use","name":"Grep","input":{"pattern":"loadConfig"}}]}}
{"type":"user","message":{"content":[{"type":"tool_result","content":"src/config.ts:12\nsrc/app.ts:3"}]}}
not json
"#;
        assert_eq!(
            render_agent_transcript(jsonl),
            "▶ Find the config loader\n\nLooking.\n\n→ Grep loadConfig\n\n  ⎿ src/config.ts:12\n    src/app.ts:3"
        );
    }
}
