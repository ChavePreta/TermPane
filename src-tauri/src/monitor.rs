use crate::model::PaneId;
use crate::state::AppState;
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::time::Duration;
use sysinfo::{Pid, ProcessRefreshKind, System, UpdateKind};
use tauri::{AppHandle, Emitter, Manager};

const TICK_MS: u64 = 250;
// Git and listening ports change slowly and cost a subprocess each; refresh
// them every this many ticks (~3 s).
const CONTEXT_EVERY: u32 = 12;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PaneContext {
    terminal_id: String,
    pane_id: String,
    git: Option<crate::model::GitInfo>,
    ports: Vec<crate::model::PortInfo>,
    dev_container: Option<String>,
    cwd: Option<String>,
}

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
        let mut git = crate::context::GitCache::default();
        let mut dev = crate::context::DevContainerCache::default();
        let docker = crate::context::start_docker_watch();
        let mut n: u32 = 0;
        loop {
            tokio::time::sleep(Duration::from_millis(TICK_MS)).await;
            n = n.wrapping_add(1);
            let slow = n % CONTEXT_EVERY == 1;
            let context = slow.then(|| (&mut git, &mut dev, docker.lock().clone()));
            if let Err(e) = tick(&app, &mut sys, context) {
                tracing::debug!("monitor tick error: {e}");
            }
        }
    });
}

fn tick(
    app: &AppHandle,
    sys: &mut System,
    slow: Option<(
        &mut crate::context::GitCache,
        &mut crate::context::DevContainerCache,
        crate::context::DockerPorts,
    )>,
) -> anyhow::Result<()> {
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
            // Back at the shell prompt: any Claude session in this pane is over.
            if command.is_none() {
                crate::claude::mark_ended(app, pane_id);
            }
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

    if let Some((git, dev, docker)) = slow {
        refresh_context(app, &state, sys, &children_of, git, dev, &docker);
        crate::claude::refresh_remote(app, |pid| sys.process(Pid::from_u32(pid)).is_some());
    }
    Ok(())
}

/// Recomputes each pane's git context and listening ports; emits
/// `pane:context` for the panes whose values changed.
fn refresh_context(
    app: &AppHandle,
    state: &AppState,
    sys: &System,
    children_of: &HashMap<u32, Vec<u32>>,
    git: &mut crate::context::GitCache,
    dev: &mut crate::context::DevContainerCache,
    docker: &crate::context::DockerPorts,
) {
    let ports_by_pid = crate::context::listening_ports();
    for (_terminal_id, pane_id, shell_pid, _) in state.all_pane_ids() {
        let cwd = crate::commands::pane_cwd(state, pane_id);
        let info = cwd.as_deref().and_then(|c| git.get(c));
        let mut ports: Vec<String> = Vec::new();
        if let Some(root) = shell_pid {
            for pid in subtree(children_of, root) {
                // Claude Code's own helper socket is not the user's server.
                if is_claude_cli(sys, pid) {
                    continue;
                }
                for p in ports_by_pid.get(&pid).into_iter().flatten() {
                    let ephemeral = crate::context::port_of(p)
                        .is_some_and(|n| n >= crate::context::EPHEMERAL_START);
                    if !ephemeral && !ports.contains(p) {
                        ports.push(p.clone());
                    }
                }
            }
        }
        // Containers of the compose project this tab works in (the listener
        // itself is Docker's process, outside the tab's process tree).
        if let Some(c) = cwd.as_deref() {
            for p in docker.for_dir(c) {
                if !ports.contains(&p) {
                    ports.push(p);
                }
            }
        }
        ports.sort_by_key(|p| crate::context::port_of(p).unwrap_or(u16::MAX));
        let devc = cwd.as_deref().and_then(|c| dev.get(c, info.as_ref().map(|g| g.toplevel.as_str())));
        let ports = crate::context::describe_ports(&ports, devc.as_ref());
        let dev_container = devc.map(|d| d.name);
        if let Some(terminal_id) = state.set_pane_context(
            pane_id,
            info.clone(),
            ports.clone(),
            dev_container.clone(),
            cwd.clone(),
        ) {
            let _ = app.emit(
                "pane:context",
                PaneContext {
                    terminal_id: terminal_id.to_string(),
                    pane_id: pane_id.to_string(),
                    git: info,
                    ports,
                    dev_container,
                    cwd,
                },
            );
        }
    }
}

fn is_claude_cli(sys: &System, pid: u32) -> bool {
    sys.process(Pid::from_u32(pid)).is_some_and(|p| {
        p.cmd()
            .first()
            .and_then(|a| std::path::Path::new(a).file_name().map(|f| f.to_os_string()))
            .is_some_and(|f| f == "claude")
    })
}

fn subtree(children_of: &HashMap<u32, Vec<u32>>, root: u32) -> Vec<u32> {
    let mut out = Vec::new();
    let mut queue: VecDeque<u32> = VecDeque::from([root]);
    while let Some(pid) = queue.pop_front() {
        if out.len() > 4096 {
            break;
        }
        out.push(pid);
        if let Some(kids) = children_of.get(&pid) {
            queue.extend(kids.iter().copied());
        }
    }
    out
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
