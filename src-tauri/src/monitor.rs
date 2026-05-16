use crate::model::PaneId;
use crate::state::AppState;
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::time::Duration;
use sysinfo::{Pid, ProcessRefreshKind, System, UpdateKind};
use tauri::{AppHandle, Emitter, Manager};

const TICK_MS: u64 = 250;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ForegroundChanged {
    terminal_id: String,
    pane_id: String,
    command: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PaneStat {
    terminal_id: String,
    pane_id: String,
    memory_bytes: u64,
}

/// Starts an async task that polls the foreground command of every pane
/// and emits `pane:foreground` when it changes.
pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut sys = System::new();
        loop {
            tokio::time::sleep(Duration::from_millis(TICK_MS)).await;
            if let Err(e) = tick(&app, &mut sys) {
                tracing::debug!("monitor tick error: {e}");
            }
        }
    });
}

fn tick(app: &AppHandle, sys: &mut System) -> anyhow::Result<()> {
    let state = app.state::<AppState>();
    let panes = state.all_pane_ids();
    if panes.is_empty() {
        return Ok(());
    }

    sys.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::new()
            .with_cmd(UpdateKind::OnlyIfNotSet)
            .with_exe(UpdateKind::OnlyIfNotSet)
            .with_memory(),
    );

    // Build a parent → children index once per tick so we can walk subtrees cheaply.
    let mut children_of: HashMap<u32, Vec<u32>> = HashMap::new();
    for (pid, proc) in sys.processes() {
        if let Some(parent) = proc.parent() {
            children_of
                .entry(parent.as_u32())
                .or_default()
                .push(pid.as_u32());
        }
    }

    let mut changes: HashMap<(String, PaneId), Option<String>> = HashMap::new();
    let mut stats: Vec<PaneStat> = Vec::with_capacity(panes.len());

    for (terminal_id, pane_id, shell_pid, fg_pgid) in panes {
        let command = match (fg_pgid, shell_pid) {
            (Some(pgid), Some(spid)) if pgid as u32 != spid => process_cmd(sys, pgid as u32),
            (Some(pgid), None) => process_cmd(sys, pgid as u32),
            _ => None,
        };

        if state.set_foreground(pane_id, command.clone()) {
            changes.insert((terminal_id.to_string(), pane_id), command);
        }

        if let Some(root) = shell_pid {
            let memory_bytes = subtree_memory(sys, &children_of, root);
            stats.push(PaneStat {
                terminal_id: terminal_id.to_string(),
                pane_id: pane_id.to_string(),
                memory_bytes,
            });
        }
    }

    for ((terminal_id, pane_id), command) in changes {
        let _ = app.emit(
            "pane:foreground",
            ForegroundChanged {
                terminal_id,
                pane_id: pane_id.to_string(),
                command,
            },
        );
    }

    if !stats.is_empty() {
        let _ = app.emit("panes:stats", stats);
    }
    Ok(())
}

/// Sums the resident memory (bytes) of `root` and every descendant in the
/// parent/child tree. BFS-bounded so a runaway process tree can't hang the tick.
fn subtree_memory(sys: &System, children_of: &HashMap<u32, Vec<u32>>, root: u32) -> u64 {
    let mut total: u64 = 0;
    let mut queue: VecDeque<u32> = VecDeque::new();
    queue.push_back(root);
    let mut visited: usize = 0;
    while let Some(pid) = queue.pop_front() {
        visited += 1;
        if visited > 4096 {
            break;
        }
        if let Some(p) = sys.process(Pid::from_u32(pid)) {
            total = total.saturating_add(p.memory());
        }
        if let Some(kids) = children_of.get(&pid) {
            for &k in kids {
                queue.push_back(k);
            }
        }
    }
    total
}

fn process_cmd(sys: &System, pid: u32) -> Option<String> {
    let p = sys.process(Pid::from_u32(pid))?;
    let cmd = p.cmd();
    if cmd.is_empty() {
        Some(p.name().to_string_lossy().to_string())
    } else {
        let joined = cmd
            .iter()
            .map(|s| s.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ");
        Some(truncate(&joined, 60))
    }
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
