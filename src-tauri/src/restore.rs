//! Save & restore tabs across real app deaths (reboot, crash, quit).
//!
//! Each instance (every window is its own process) rewrites
//! `sessions/<pid>.json` whenever its tabs change, and holds an exclusive
//! `flock` on `sessions/<pid>.lock` for its whole lifetime. On launch, any
//! session file whose lock can be taken belongs to an instance that is gone, so
//! it is claimed (restored into this window) and deleted. A live second window
//! keeps its lock, so its tabs are never duplicated.
//!
//! Restored panes get a fresh shell in the saved directory; a pane that was
//! running Claude Code gets `claude --resume <session>` typed into it.

use crate::model::{SplitDir, TerminalId};
use crate::state::{AppState, LayoutNode, PaneData, Terminal};
use chrono::Utc;
use nix::fcntl::{Flock, FlockArg};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;
use tauri::{AppHandle, Manager};

#[derive(Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SavedWorkspace {
    pub terminals: Vec<SavedTerminal>,
    pub active: Option<usize>,
}

#[derive(Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SavedTerminal {
    pub label: String,
    /// Index of the parent in `SavedWorkspace::terminals` (always earlier).
    pub parent: Option<usize>,
    pub marked_unread: bool,
    #[serde(default = "default_true")]
    pub auto_label: bool,
    pub layout: SavedLayout,
}

fn default_true() -> bool {
    true
}

#[derive(Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SavedLayout {
    Leaf {
        cwd: Option<String>,
        claude_session: Option<String>,
        #[serde(skip)]
        shell_pid: Option<u32>,
    },
    Split {
        dir: SplitDir,
        ratios: Vec<f32>,
        children: Vec<SavedLayout>,
    },
}

fn sessions_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("TermPane").join("sessions"))
}

static OWN_LOCK: OnceLock<Flock<File>> = OnceLock::new();

/// Takes this instance's lock and starts the background saver.
pub fn start_saver(app: AppHandle) {
    let Some(dir) = sessions_dir() else { return };
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let pid = std::process::id();
    if let Ok(f) = File::create(dir.join(format!("{pid}.lock"))) {
        if let Ok(lock) = Flock::lock(f, FlockArg::LockExclusiveNonblock) {
            let _ = OWN_LOCK.set(lock);
        }
    }
    let path = dir.join(format!("{pid}.json"));
    std::thread::Builder::new()
        .name("session-saver".into())
        .spawn(move || {
            let mut last: Option<String> = None;
            loop {
                std::thread::sleep(Duration::from_secs(3));
                let ws = capture(&app.state::<AppState>());
                let Ok(json) = serde_json::to_string_pretty(&ws) else { continue };
                if last.as_deref() == Some(json.as_str()) {
                    continue;
                }
                let tmp = path.with_extension("json.tmp");
                if std::fs::write(&tmp, &json).and_then(|_| std::fs::rename(&tmp, &path)).is_ok() {
                    last = Some(json);
                }
            }
        })
        .expect("failed to start session saver");
}

fn capture(state: &AppState) -> SavedWorkspace {
    let mut ws = state.with_terminals(|terminals, active| {
        let index: HashMap<TerminalId, usize> =
            terminals.iter().enumerate().map(|(i, t)| (t.id, i)).collect();
        SavedWorkspace {
            terminals: terminals
                .iter()
                .map(|t| SavedTerminal {
                    label: t.label.clone(),
                    parent: t.parent_id.and_then(|p| index.get(&p).copied()),
                    marked_unread: t.marked_unread,
                    auto_label: t.auto_label,
                    layout: save_layout(&t.layout, t),
                })
                .collect(),
            active: active.and_then(|a| index.get(&a).copied()),
        }
    });
    for t in ws.terminals.iter_mut() {
        fill_cwds(&mut t.layout);
    }
    ws
}

fn save_layout(node: &LayoutNode, t: &Terminal) -> SavedLayout {
    match node {
        LayoutNode::Leaf(id) => {
            let pane = t.panes.get(id);
            let live = pane.and_then(|p| p.claude.as_ref()).filter(|c| c.is_live());
            SavedLayout::Leaf {
                cwd: live.and_then(|c| c.cwd.clone()),
                claude_session: live.map(|c| c.session_id.clone()),
                shell_pid: pane.and_then(|p| p.pty.shell_pid),
            }
        }
        LayoutNode::Split { dir, children, ratios } => SavedLayout::Split {
            dir: *dir,
            ratios: ratios.clone(),
            children: children.iter().map(|c| save_layout(c, t)).collect(),
        },
    }
}

fn fill_cwds(node: &mut SavedLayout) {
    match node {
        SavedLayout::Leaf { cwd, shell_pid, .. } => {
            if cwd.is_none() {
                *cwd = shell_pid.and_then(crate::commands::cwd_of_pid);
            }
        }
        SavedLayout::Split { children, .. } => children.iter_mut().for_each(fill_cwds),
    }
}

/// Claims the session files of instances that are no longer running. With
/// `restore` off they are discarded instead.
pub fn claim_orphans(restore: bool) -> Vec<SavedWorkspace> {
    let Some(dir) = sessions_dir() else { return Vec::new() };
    let Ok(entries) = std::fs::read_dir(&dir) else { return Vec::new() };
    let own = std::process::id().to_string();
    let mut out = Vec::new();
    for e in entries.flatten() {
        let path = e.path();
        if path.extension().is_none_or(|x| x != "lock") {
            continue;
        }
        if path.file_stem().is_some_and(|s| s.to_string_lossy() == own) {
            continue;
        }
        let Ok(f) = File::options().read(true).write(true).open(&path) else { continue };
        let Ok(lock) = Flock::lock(f, FlockArg::LockExclusiveNonblock) else { continue };
        let json_path = path.with_extension("json");
        if restore {
            if let Some(ws) = std::fs::read_to_string(&json_path)
                .ok()
                .and_then(|s| serde_json::from_str::<SavedWorkspace>(&s).ok())
            {
                if !ws.terminals.is_empty() {
                    out.push(ws);
                }
            }
        }
        let _ = std::fs::remove_file(&json_path);
        let _ = std::fs::remove_file(&path);
        drop(lock);
    }
    out
}

/// Recreates saved tabs in this instance. Returns how many were restored.
pub fn restore(app: &AppHandle, workspaces: Vec<SavedWorkspace>) -> usize {
    let state = app.state::<AppState>();
    let mut restored = 0;
    let mut first_active: Option<TerminalId> = None;
    for ws in workspaces {
        let mut ids: Vec<Option<TerminalId>> = Vec::with_capacity(ws.terminals.len());
        for (i, saved) in ws.terminals.into_iter().enumerate() {
            let mut panes = HashMap::new();
            let Some(layout) = build_layout(app, &saved.layout, &mut panes) else {
                ids.push(None);
                continue;
            };
            let active_pane = *layout.leaves().first().expect("layout has a leaf");
            let id = TerminalId::new();
            state.insert(Terminal {
                id,
                label: saved.label,
                created_at: Utc::now(),
                panes,
                layout,
                active_pane,
                parent_id: saved.parent.and_then(|p| ids.get(p).copied().flatten()),
                marked_unread: saved.marked_unread,
                auto_label: saved.auto_label,
            });
            if first_active.is_none() && ws.active == Some(i) {
                first_active = Some(id);
            }
            ids.push(Some(id));
            restored += 1;
        }
    }
    if let Some(a) = first_active {
        state.set_active(a);
    }
    restored
}

fn build_layout(
    app: &AppHandle,
    saved: &SavedLayout,
    panes: &mut HashMap<crate::model::PaneId, PaneData>,
) -> Option<LayoutNode> {
    match saved {
        SavedLayout::Leaf { cwd, claude_session, .. } => {
            let pane_id = crate::model::PaneId::new();
            let cwd = cwd.as_deref().filter(|c| std::path::Path::new(c).is_dir());
            let pty = crate::commands::spawn_pty(app, pane_id, cwd, None).ok()?;
            if let Some(session) = claude_session {
                pty.set_startup_command(format!("claude --resume {session}"));
            }
            panes.insert(pane_id, PaneData::new(pty));
            Some(LayoutNode::Leaf(pane_id))
        }
        SavedLayout::Split { dir, ratios, children } => {
            let built: Vec<LayoutNode> =
                children.iter().filter_map(|c| build_layout(app, c, panes)).collect();
            match built.len() {
                0 => None,
                1 => built.into_iter().next(),
                n => Some(LayoutNode::Split {
                    dir: *dir,
                    ratios: if ratios.len() == n { ratios.clone() } else { vec![1.0 / n as f32; n] },
                    children: built,
                }),
            }
        }
    }
}
