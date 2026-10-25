use crate::model::{PaneId, SplitDir, TerminalId, TerminalSnapshot};
use crate::preferences::{self, Preferences};
use crate::pty::{self, PtyHandle};
use crate::shortcuts;
use crate::state::{AppState, CloseResult, ExtractResult, LayoutNode, MergeResult, PaneData, Terminal};
use base64::Engine;
use parking_lot::Mutex;
use serde::Serialize;
use chrono::Utc;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};

fn parse_terminal_id(s: &str) -> Result<TerminalId, String> {
    TerminalId::from_str(s).map_err(|e| format!("invalid terminal_id: {e}"))
}

fn parse_pane_id(s: &str) -> Result<PaneId, String> {
    PaneId::from_str(s).map_err(|e| format!("invalid pane_id: {e}"))
}

/// Resolve a process's current working directory via sysinfo.
/// Used so a new terminal/split inherits the cwd of the shell that spawned it.
pub(crate) fn cwd_of_pid(pid: u32) -> Option<String> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let mut sys = System::new();
    let target = Pid::from_u32(pid);
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[target]),
        true,
        ProcessRefreshKind::new().with_cwd(UpdateKind::Always),
    );
    let p = sys.process(target)?;
    Some(p.cwd()?.to_string_lossy().into_owned())
}

/// The directory a new tab/split opened "from" this pane should start in.
/// While Claude Code runs in the pane, that's Claude's own cwd (it may have
/// moved into a git worktree); otherwise the shell's.
pub(crate) fn pane_cwd(state: &AppState, pane_id: PaneId) -> Option<String> {
    let claude_cwd = state
        .claude_of(pane_id)
        .filter(|c| c.is_live())
        .and_then(|c| c.cwd)
        .filter(|c| std::path::Path::new(c).is_dir());
    claude_cwd.or_else(|| state.shell_pid_of(pane_id).and_then(cwd_of_pid))
}

pub(crate) fn spawn_pty(
    app: &AppHandle,
    pane_id: PaneId,
    cwd: Option<&str>,
    shell: Option<&str>,
) -> anyhow::Result<Arc<PtyHandle>> {
    // Lets `termpane hook` / `termpane mcp` running inside this shell find this
    // instance and know which pane they belong to.
    let env = [
        (crate::control::ENV_PANE, pane_id.to_string()),
        (
            crate::control::ENV_SOCKET,
            crate::control::own_socket_path().to_string_lossy().into_owned(),
        ),
        (crate::control::ENV_BIN, crate::claude::exe_path()),
    ];
    let pty::SpawnResult {
        master,
        writer,
        reader,
        child,
        pid,
    } = pty::spawn_shell(cwd, shell, &env)?;
    let backlog = Arc::new(Mutex::new(pty::Backlog::new()));
    let reader_thread =
        pty::io::start_reader_thread(app.clone(), pane_id, backlog.clone(), reader);
    Ok(Arc::new(PtyHandle::new(
        master,
        writer,
        child,
        pid,
        backlog,
        reader_thread,
    )))
}

#[tauri::command]
pub fn list_terminals(state: tauri::State<'_, AppState>) -> Vec<TerminalSnapshot> {
    state.snapshots()
}

#[tauri::command]
pub fn active_terminal(state: tauri::State<'_, AppState>) -> Option<String> {
    state.active().map(|id| id.to_string())
}

#[tauri::command]
pub fn set_active_terminal(
    state: tauri::State<'_, AppState>,
    terminal_id: String,
) -> Result<(), String> {
    let id = parse_terminal_id(&terminal_id)?;
    if state.set_active(id) {
        Ok(())
    } else {
        Err(format!("terminal {} not found", id))
    }
}

#[tauri::command]
pub fn open_terminal(
    app: AppHandle,
    cwd: Option<String>,
    command: Option<String>,
    shell: Option<String>,
    name: Option<String>,
    parent_id: Option<String>,
    auto_label: Option<bool>,
) -> Result<TerminalSnapshot, String> {
    let parent = parent_id.as_deref().map(parse_terminal_id).transpose()?;
    open_terminal_impl(
        &app,
        cwd.as_deref(),
        command.as_deref(),
        shell.as_deref(),
        name.as_deref(),
        parent,
        auto_label,
    )
    .map_err(|e| e.to_string())
}

/// Expands a leading `~` / `~/` to the user's HOME. CommandBuilder::cwd takes a
/// literal path, so without this `~/Projetos/...` would not resolve.
fn expand_tilde(path: &str) -> String {
    if path == "~" || path.starts_with("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            let home = home.to_string_lossy();
            return format!("{}{}", home, &path[1..]);
        }
    }
    path.to_string()
}

pub fn open_terminal_impl(
    app: &AppHandle,
    cwd: Option<&str>,
    command: Option<&str>,
    shell: Option<&str>,
    name: Option<&str>,
    parent: Option<TerminalId>,
    // Whether the label may later follow the title of a Claude session run in
    // the tab. On by default — a shortcut's name only says what opened the
    // tab, and a tab where Claude never runs keeps it anyway; only a rename
    // by the user (or an explicit `Some(false)`, like a fork's) pins it.
    auto_label: Option<bool>,
) -> anyhow::Result<TerminalSnapshot> {
    let auto_label = auto_label.unwrap_or(true);
    let state = app.state::<AppState>();
    // A sub-tab starts where its parent tab is working; anything else where the
    // active pane is.
    let origin_pane = match parent {
        Some(p) => state.active_pane_of(p),
        None => state.active_pane(),
    };
    let inherited_cwd = || origin_pane.and_then(|p| pane_cwd(&state, p));
    let terminal_id = TerminalId::new();
    let pane_id = PaneId::new();
    // A custom shortcut carries a name — label the card with it verbatim (no
    // "#<seq>" suffix). A plain "+ New terminal" (no name) gets the sequenced
    // "Term#N" label. Labels are display-only (never used as keys), so repeated
    // opens of the same shortcut sharing a label is fine.
    let label = match name.map(str::trim).filter(|s| !s.is_empty()) {
        Some(n) => n.to_string(),
        None => state.allocate_label("Term"),
    };
    let created_at = Utc::now();

    let resolved_cwd = match cwd {
        Some(c) => {
            let expanded = expand_tilde(c);
            // A bogus cwd (a typo, or an unsupported `~user/…`) would make the
            // PTY spawn fail or land in a surprising directory with no feedback.
            // Validate it, falling back to the active pane's cwd if it's not a
            // real directory.
            if std::path::Path::new(&expanded).is_dir() {
                Some(expanded)
            } else {
                tracing::warn!("requested cwd is not a directory, ignoring: {expanded}");
                inherited_cwd()
            }
        }
        None => inherited_cwd(),
    };
    let pty = spawn_pty(app, pane_id, resolved_cwd.as_deref(), shell)?;

    // Stash the optional startup command instead of writing it now. The frontend
    // flushes it (via take_startup_command) once its TerminalView has subscribed
    // to this pane's output — Tauri doesn't replay events to a late listener, so
    // writing it here could lose the command's echo and early output.
    if let Some(cmd) = command {
        let cmd = cmd.trim();
        if !cmd.is_empty() {
            pty.set_startup_command(cmd.to_string());
        }
    }

    let mut panes = HashMap::new();
    panes.insert(pane_id, PaneData::new(pty));
    let terminal = Terminal {
        id: terminal_id,
        label,
        created_at,
        panes,
        layout: LayoutNode::Leaf(pane_id),
        active_pane: pane_id,
        parent_id: parent,
        marked_unread: false,
        auto_label,
    };
    state.insert(terminal);
    let snapshot = state
        .terminal_snapshot(terminal_id)
        .ok_or_else(|| anyhow::anyhow!("terminal disappeared after insert"))?;

    let _ = app.emit("terminal:added", snapshot.clone());
    // A sub-tab lands mid-list, not at the end.
    if parent.is_some() {
        let _ = app.emit("terminals:reordered", state.terminal_order());
    }
    Ok(snapshot)
}

/// Pops the one-shot startup command for a pane (set by a custom shortcut),
/// returning `None` if there is none. The frontend calls this once its
/// TerminalView has subscribed to the pane's output, then writes the command.
#[tauri::command]
pub fn take_startup_command(
    state: tauri::State<'_, AppState>,
    pane_id: String,
) -> Result<Option<String>, String> {
    let pid = parse_pane_id(&pane_id)?;
    Ok(state.pty_of(pid).and_then(|p| p.take_startup_command()))
}

#[tauri::command]
pub fn close_terminal(app: AppHandle, terminal_id: String) -> Result<(), String> {
    let id = parse_terminal_id(&terminal_id)?;
    close_terminal_impl(&app, id)
}

/// Removes a terminal and kills its panes. Its sub-tabs are promoted one level
/// up, so they get a fresh snapshot.
pub fn close_terminal_impl(app: &AppHandle, id: TerminalId) -> Result<(), String> {
    let state = app.state::<AppState>();
    let children = state.children_of(id);
    let term = state
        .remove_terminal(id)
        .ok_or_else(|| format!("terminal {} not found", id))?;
    for (_, p) in term.panes {
        p.pty.kill();
    }
    let _ = app.emit("terminal:removed", id.to_string());
    emit_updated(app, &state, &children);
    Ok(())
}

fn emit_updated(app: &AppHandle, state: &AppState, ids: &[TerminalId]) {
    for id in ids {
        if let Some(s) = state.terminal_snapshot(*id) {
            let _ = app.emit("terminal:updated", s);
        }
    }
}

#[tauri::command]
pub fn split_pane(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    pane_id: String,
    direction: String,
) -> Result<TerminalSnapshot, String> {
    let pid = parse_pane_id(&pane_id)?;
    let dir = match direction.as_str() {
        "horizontal" => SplitDir::Horizontal,
        "vertical" => SplitDir::Vertical,
        _ => return Err(format!("invalid direction: {direction}")),
    };
    let new_pane = PaneId::new();
    let inherited_cwd = pane_cwd(&state, pid);
    let new_pty =
        spawn_pty(&app, new_pane, inherited_cwd.as_deref(), None).map_err(|e| e.to_string())?;
    let terminal_id = state
        .split(pid, new_pane, new_pty, dir)
        .ok_or_else(|| format!("pane {} not found", pid))?;
    let snap = state
        .terminal_snapshot(terminal_id)
        .ok_or_else(|| "terminal disappeared after split".to_string())?;
    let _ = app.emit("terminal:updated", snap.clone());
    Ok(snap)
}

#[tauri::command]
pub fn close_pane(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    pane_id: String,
) -> Result<(), String> {
    let pid = parse_pane_id(&pane_id)?;
    match state.close_pane(pid) {
        CloseResult::NotFound => Err(format!("pane {} not found", pid)),
        CloseResult::PaneRemoved { terminal_id, removed_pane } => {
            if let Some(p) = removed_pane {
                p.pty.kill();
            }
            let snap = state.terminal_snapshot(terminal_id);
            if let Some(s) = snap {
                let _ = app.emit("terminal:updated", s);
            }
            Ok(())
        }
        CloseResult::TerminalEmpty { terminal_id, removed_pane } => {
            if let Some(p) = removed_pane {
                p.pty.kill();
            }
            let _ = close_terminal_impl(&app, terminal_id);
            Ok(())
        }
    }
}

#[tauri::command]
pub fn focus_pane(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    pane_id: String,
) -> Result<(), String> {
    let pid = parse_pane_id(&pane_id)?;
    let terminal_id = state
        .focus_pane(pid)
        .ok_or_else(|| format!("pane {} not found", pid))?;
    let snap = state.terminal_snapshot(terminal_id);
    if let Some(s) = snap {
        let _ = app.emit("terminal:updated", s);
    }
    Ok(())
}

#[tauri::command]
pub fn write_input(
    state: tauri::State<'_, AppState>,
    pane_id: String,
    data: String,
) -> Result<(), String> {
    let pid = parse_pane_id(&pane_id)?;
    let pty = state
        .pty_of(pid)
        .ok_or_else(|| format!("pane {} not found", pid))?;
    pty.write_all(data.as_bytes())
        .map_err(|e| format!("write failed: {e}"))
}

#[tauri::command]
pub fn resize_pane(
    state: tauri::State<'_, AppState>,
    pane_id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    let pid = parse_pane_id(&pane_id)?;
    let pty = state
        .pty_of(pid)
        .ok_or_else(|| format!("pane {} not found", pid))?;
    pty.resize(cols, rows).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_layout_ratios(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    terminal_id: String,
    path: Vec<u32>,
    ratios: Vec<f32>,
) -> Result<(), String> {
    let tid = parse_terminal_id(&terminal_id)?;
    let path_usize: Vec<usize> = path.into_iter().map(|x| x as usize).collect();
    if !state.set_layout_ratios(tid, path_usize, ratios) {
        return Err("invalid path/ratios".to_string());
    }
    if let Some(s) = state.terminal_snapshot(tid) {
        let _ = app.emit("terminal:updated", s);
    }
    Ok(())
}

#[tauri::command]
pub fn rename_terminal(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    terminal_id: String,
    label: String,
) -> Result<TerminalSnapshot, String> {
    let id = parse_terminal_id(&terminal_id)?;
    let trimmed = label.trim();
    if trimmed.is_empty() {
        return Err("empty label".into());
    }
    if !state.rename_terminal(id, trimmed.to_string(), true) {
        return Err(format!("terminal {} not found", id));
    }
    // A name the user chose also names the Claude sessions running in the tab
    // (a longer form, with the project, for the /resume picker).
    crate::claude::name_sessions_after_tab(&state, id, trimmed);
    let snap = state
        .terminal_snapshot(id)
        .ok_or_else(|| "terminal disappeared after rename".to_string())?;
    let _ = app.emit("terminal:updated", snap.clone());
    Ok(snap)
}

#[tauri::command]
pub fn reorder_terminals(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    ids: Vec<String>,
) -> Result<(), String> {
    let parsed: Result<Vec<TerminalId>, _> = ids.iter().map(|s| parse_terminal_id(s)).collect();
    let parsed = parsed?;
    if !state.reorder(&parsed) {
        return Err("provided ids do not match current terminals".into());
    }
    let _ = app.emit("terminals:reordered", state.terminal_order());
    Ok(())
}

/// Drag-and-drop in the sidebar: re-parent a terminal (None = top level) and
/// apply the new display order in one step.
#[tauri::command]
pub fn move_terminal(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    terminal_id: String,
    parent_id: Option<String>,
    order: Vec<String>,
) -> Result<(), String> {
    let id = parse_terminal_id(&terminal_id)?;
    let parent = parent_id.as_deref().map(parse_terminal_id).transpose()?;
    let order: Vec<TerminalId> = order
        .iter()
        .map(|s| parse_terminal_id(s))
        .collect::<Result<_, _>>()?;
    if !state.move_terminal(id, parent, &order) {
        return Err("invalid move".into());
    }
    emit_updated(&app, &state, &[id]);
    let _ = app.emit("terminals:reordered", state.terminal_order());
    Ok(())
}

#[tauri::command]
pub fn set_terminal_unread(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    terminal_id: String,
    unread: bool,
) -> Result<(), String> {
    let id = parse_terminal_id(&terminal_id)?;
    if !state.set_unread(id, unread) {
        return Err(format!("terminal {} not found", id));
    }
    emit_updated(&app, &state, &[id]);
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaneBacklog {
    data: String,
    /// Stream offset just past `data`; live chunks starting before it are dupes.
    end: u64,
}

/// A pane's recent raw output, replayed by a freshly mounted view (e.g. after
/// the webview was reloaded) so its scrollback survives.
#[tauri::command]
pub fn get_pane_backlog(
    state: tauri::State<'_, AppState>,
    pane_id: String,
) -> Result<PaneBacklog, String> {
    let pid = parse_pane_id(&pane_id)?;
    let pty = state.pty_of(pid).ok_or_else(|| format!("pane {} not found", pid))?;
    let (bytes, end) = pty.backlog.lock().snapshot();
    Ok(PaneBacklog {
        data: base64::engine::general_purpose::STANDARD.encode(bytes),
        end,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionFiles {
    pane_id: String,
    session_id: String,
    live: bool,
    files: Vec<String>,
    shared: Vec<String>,
}

/// Files written by each Claude session in a tab, flagging those another
/// session in this window also wrote.
#[tauri::command]
pub fn get_terminal_files(
    state: tauri::State<'_, AppState>,
    terminal_id: String,
) -> Result<Vec<SessionFiles>, String> {
    let id = parse_terminal_id(&terminal_id)?;
    let snap = state.terminal_snapshot(id).ok_or("terminal not found")?;
    Ok(snap
        .panes
        .iter()
        .filter_map(|p| {
            let c = p.claude.as_ref()?;
            let pane = p.id.parse::<PaneId>().ok()?;
            Some(SessionFiles {
                pane_id: p.id.clone(),
                session_id: c.session_id.clone(),
                live: c.is_live(),
                files: c.files.clone(),
                shared: crate::claude::shared_files(&state, pane),
            })
        })
        .collect())
}

/// `git add` the files this tab's Claude sessions wrote, except files another
/// session also wrote.
#[tauri::command]
pub fn stage_terminal_files(
    state: tauri::State<'_, AppState>,
    terminal_id: String,
) -> Result<crate::claude::StageResult, String> {
    let id = parse_terminal_id(&terminal_id)?;
    let snap = state.terminal_snapshot(id).ok_or("terminal not found")?;
    let mut total = crate::claude::StageResult::default();
    for p in &snap.panes {
        let Ok(pane) = p.id.parse::<PaneId>() else { continue };
        let r = crate::claude::stage_files(&state, pane);
        total.staged.extend(r.staged);
        total.shared.extend(r.shared);
        total.errors.extend(r.errors);
    }
    Ok(total)
}

#[tauri::command]
pub fn clear_terminal_files(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    terminal_id: String,
) -> Result<(), String> {
    let id = parse_terminal_id(&terminal_id)?;
    let snap = state.terminal_snapshot(id).ok_or("terminal not found")?;
    for p in &snap.panes {
        if let Ok(pane) = p.id.parse::<PaneId>() {
            crate::claude::clear_files(&app, pane);
        }
    }
    Ok(())
}

#[tauri::command]
pub fn read_background_output(
    state: tauri::State<'_, AppState>,
    pane_id: String,
    task_id: String,
) -> Result<crate::claude::BackgroundOutput, String> {
    let pane = parse_pane_id(&pane_id)?;
    crate::claude::read_background(&state, pane, &task_id)
}

#[tauri::command]
pub async fn file_diff(path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || crate::claude::file_diff(&path))
        .await
        .map_err(|e| e.to_string())?
}

/// A clickable path in a pane's output: relative paths resolve against the
/// pane's working directory (Claude's while it runs there).
#[tauri::command]
pub async fn open_path(
    app: AppHandle,
    pane_id: String,
    path: String,
    line: Option<u32>,
    col: Option<u32>,
) -> Result<(), String> {
    let pane = parse_pane_id(&pane_id)?;
    let cwd = pane_cwd(&app.state::<AppState>(), pane);
    tauri::async_runtime::spawn_blocking(move || {
        crate::claude::open_in_editor(&path, line, col, cwd.as_deref())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Opens a sub-tab with a shell inside the dev container of the pane's
/// project (devcontainer.json `dockerComposeFile` + `service`), at its
/// workspace folder, starting the service only if it isn't running.
#[tauri::command]
pub fn open_devcontainer_shell(app: AppHandle, pane_id: String) -> Result<TerminalSnapshot, String> {
    let pane = parse_pane_id(&pane_id)?;
    let state = app.state::<AppState>();
    let cwd = pane_cwd(&state, pane).ok_or("can't tell this pane's folder")?;
    let toplevel = crate::context::git_info(&cwd).map(|g| g.toplevel);
    let dc = crate::devcontainer::find(std::path::Path::new(&cwd), toplevel.as_deref().map(std::path::Path::new))
        .ok_or("no devcontainer.json for this project")?;
    let command = crate::devcontainer::shell_command(&dc)
        .ok_or("only Docker Compose dev containers (dockerComposeFile + service) are supported")?;
    let parent = state.terminal_of_pane(pane);
    open_terminal_impl(
        &app,
        Some(&dc.root.to_string_lossy()),
        Some(&command),
        None,
        Some(&format!("{} · container", dc.name)),
        parent,
        Some(false),
    )
    .map_err(|e| e.to_string())
}

/// Opens a server link from a tab card. Only a url TermPane itself derived for
/// that tab (its ports list) is opened — the page can't use this to open an
/// arbitrary address, which keeps the opener's per-URL scoping meaningful
/// while still allowing hosts a project names in its devcontainer.json.
#[tauri::command]
pub fn open_port_url(
    state: tauri::State<'_, AppState>,
    terminal_id: String,
    url: String,
) -> Result<(), String> {
    let id = parse_terminal_id(&terminal_id)?;
    let snap = state.terminal_snapshot(id).ok_or("terminal not found")?;
    let known = snap
        .panes
        .iter()
        .flat_map(|p| p.ports.iter())
        .any(|p| p.url.as_deref() == Some(url.as_str()));
    if !known {
        return Err("not one of this tab's server links".into());
    }
    let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    std::process::Command::new(opener)
        .arg(&url)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("{opener}: {e}"))
}

/// Dock / taskbar badge: how many tabs need the user. 0 clears it.
#[tauri::command]
pub fn set_badge_count(app: AppHandle, count: u32) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "window 'main' not found".to_string())?;
    window
        .set_badge_count((count > 0).then_some(count as i64))
        .map_err(|e| e.to_string())
}

/// The pane menu's "Fork Claude session": same as `/fork` inside TermPane.
#[tauri::command]
pub fn fork_claude_session(app: AppHandle, pane_id: String) -> Result<String, String> {
    let pane = parse_pane_id(&pane_id)?;
    crate::control::fork_session(&app, pane, &serde_json::json!({})).map(|(_, label)| label)
}

#[tauri::command]
pub async fn list_claude_sessions() -> Vec<crate::claude::ClaudeSession> {
    tauri::async_runtime::spawn_blocking(crate::claude::list_sessions)
        .await
        .unwrap_or_default()
}

/// Working directory of the active pane (Claude's, when it runs there).
#[tauri::command]
pub fn active_cwd(state: tauri::State<'_, AppState>) -> Option<String> {
    state.active_pane().and_then(|p| pane_cwd(&state, p))
}

#[tauri::command]
pub async fn claude_integration_status() -> Result<crate::claude::IntegrationStatus, String> {
    tauri::async_runtime::spawn_blocking(crate::claude::integration_status)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn install_claude_plugin() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(crate::claude::install_plugin)
        .await
        .map_err(|e| e.to_string())?
}

/// Reloads the webview. Lossless: every view replays its pane's backlog.
#[tauri::command]
pub fn redraw_terminals(app: AppHandle) {
    redraw_all(&app);
}

pub fn redraw_all(app: &AppHandle) {
    for w in app.webview_windows().into_values() {
        let _ = w.eval("window.location.reload()");
    }
}

#[tauri::command]
pub fn merge_terminal_into_pane(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    source_terminal_id: String,
    target_pane_id: String,
    direction: String,
) -> Result<TerminalSnapshot, String> {
    let src_id = parse_terminal_id(&source_terminal_id)?;
    let tgt_pane = parse_pane_id(&target_pane_id)?;
    let dir = match direction.as_str() {
        "horizontal" => SplitDir::Horizontal,
        "vertical" => SplitDir::Vertical,
        _ => return Err(format!("invalid direction: {direction}")),
    };
    let MergeResult {
        target_terminal_id,
        removed_source_id,
    } = state
        .merge_terminal_into_pane(src_id, tgt_pane, dir)
        .ok_or_else(|| "cannot merge: source or target not found, or same terminal".to_string())?;

    let target_snap = state
        .terminal_snapshot(target_terminal_id)
        .ok_or_else(|| "target terminal disappeared after merge".to_string())?;
    let _ = app.emit("terminal:updated", target_snap.clone());
    let _ = app.emit("terminal:removed", removed_source_id.to_string());
    Ok(target_snap)
}

#[tauri::command]
pub fn flip_parent_split(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    pane_id: String,
) -> Result<TerminalSnapshot, String> {
    let pid = parse_pane_id(&pane_id)?;
    let terminal_id = state
        .flip_parent_split(pid)
        .ok_or_else(|| "no parent split to flip".to_string())?;
    let snap = state
        .terminal_snapshot(terminal_id)
        .ok_or_else(|| "terminal disappeared after flip".to_string())?;
    let _ = app.emit("terminal:updated", snap.clone());
    Ok(snap)
}

#[tauri::command]
pub fn write_input_broadcast(
    state: tauri::State<'_, AppState>,
    terminal_id: String,
    data: String,
) -> Result<(), String> {
    let tid = parse_terminal_id(&terminal_id)?;
    let ptys = state.ptys_of_terminal(tid);
    if ptys.is_empty() {
        return Err(format!("terminal {} has no live panes", tid));
    }
    let bytes = data.as_bytes();
    for pty in ptys {
        if let Err(e) = pty.write_all(bytes) {
            tracing::warn!("broadcast write failed: {e}");
        }
    }
    Ok(())
}

#[tauri::command]
pub fn get_platform() -> String {
    std::env::consts::OS.to_string()
}

/// The frontend painted a frame after a system wake (called from inside a
/// `requestAnimationFrame`), proving the WebContent process is alive — cancel
/// the pending native reload for this wake generation. See `wake.rs`.
#[tauri::command]
pub fn ack_woke(state: tauri::State<'_, Arc<crate::wake::WakeWatch>>, generation: u64) {
    state.ack(generation);
}

#[tauri::command]
pub fn extract_pane(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    pane_id: String,
) -> Result<TerminalSnapshot, String> {
    let pid = parse_pane_id(&pane_id)?;
    let new_tid = TerminalId::new();
    let label = state.allocate_label("Term");
    let ExtractResult { source_terminal_id } = state
        .extract_pane(pid, new_tid, label)
        .ok_or_else(|| {
            "cannot extract: pane not found or sole pane of its terminal".to_string()
        })?;

    let src_snap = state.terminal_snapshot(source_terminal_id);
    let new_snap = state
        .terminal_snapshot(new_tid)
        .ok_or_else(|| "new terminal disappeared after extract".to_string())?;

    if let Some(s) = src_snap {
        let _ = app.emit("terminal:updated", s);
    }
    let _ = app.emit("terminal:added", new_snap.clone());
    Ok(new_snap)
}

#[tauri::command]
pub fn set_always_on_top(app: AppHandle, enabled: bool) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "window 'main' not found".to_string())?;
    window
        .set_always_on_top(enabled)
        .map_err(|e| format!("set_always_on_top failed: {e}"))
}

/// Called by the frontend after the user confirms the quit dialog. Bypasses
/// the `CloseRequested` interception (which would otherwise re-trigger the
/// dialog) by destroying the window directly.
#[tauri::command]
pub fn quit_app(app: AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "window 'main' not found".to_string())?;
    window
        .destroy()
        .map_err(|e| format!("destroy failed: {e}"))
}

#[tauri::command]
pub fn get_preferences() -> Preferences {
    preferences::load()
}

#[tauri::command]
pub fn set_preferences(app: AppHandle, prefs: Preferences) -> Result<Preferences, String> {
    let sanitized = preferences::sanitize(prefs);
    preferences::save(&sanitized).map_err(|e| format!("save failed: {e}"))?;
    let _ = app.emit("preferences:changed", sanitized.clone());
    Ok(sanitized)
}

#[tauri::command]
pub fn get_shortcuts() -> shortcuts::Shortcuts {
    shortcuts::load()
}

#[tauri::command]
pub fn set_shortcuts(
    app: AppHandle,
    shortcuts: shortcuts::Shortcuts,
) -> Result<shortcuts::Shortcuts, String> {
    let sanitized = shortcuts::sanitize(shortcuts);
    shortcuts::save(&sanitized).map_err(|e| format!("save failed: {e}"))?;
    let _ = app.emit("shortcuts:changed", sanitized.clone());
    Ok(sanitized)
}

/// Called once from the Tauri Builder `setup` hook. Starts the control socket,
/// restores tabs left by instances that are gone (or opens Term#1), then starts
/// the session saver and the foreground monitor.
pub fn bootstrap(app: AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    crate::control::start(app.clone());
    crate::claude::refresh_installed_plugin();
    let restore = preferences::load().restore_sessions;
    let saved = crate::restore::claim_orphans(restore);
    if crate::restore::restore(&app, saved) == 0 {
        open_terminal_impl(&app, None, None, None, None, None, None)?;
    }
    crate::restore::start_saver(app.clone());
    crate::monitor::start(app);
    Ok(())
}

