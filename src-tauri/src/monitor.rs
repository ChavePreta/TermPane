use crate::model::PaneId;
use crate::state::AppState;
use serde::Serialize;
use std::collections::HashMap;
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
            .with_exe(UpdateKind::OnlyIfNotSet),
    );

    let mut changes: HashMap<(String, PaneId), Option<String>> = HashMap::new();
    for (terminal_id, pane_id, shell_pid, fg_pgid) in panes {
        let command = match (fg_pgid, shell_pid) {
            (Some(pgid), Some(spid)) if pgid as u32 != spid => process_cmd(sys, pgid as u32),
            (Some(pgid), None) => process_cmd(sys, pgid as u32),
            _ => None,
        };

        if state.set_foreground(pane_id, command.clone()) {
            changes.insert((terminal_id.to_string(), pane_id), command);
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
    Ok(())
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
