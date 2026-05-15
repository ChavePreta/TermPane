use crate::model::{PaneId, SplitDir, TerminalId, TerminalSnapshot};
use crate::preferences::{self, Preferences};
use crate::pty::{self, PtyHandle};
use crate::state::{AppState, CloseResult, LayoutNode, PaneData, Terminal};
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

fn spawn_pty(app: &AppHandle, pane_id: PaneId, cwd: Option<&str>) -> anyhow::Result<Arc<PtyHandle>> {
    let pty::SpawnResult {
        master,
        writer,
        reader,
        child,
        pid,
    } = pty::spawn_shell(cwd)?;
    let reader_thread = pty::io::start_reader_thread(app.clone(), pane_id, reader);
    Ok(Arc::new(PtyHandle::new(
        master,
        writer,
        child,
        pid,
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
pub fn open_terminal(app: AppHandle) -> Result<TerminalSnapshot, String> {
    open_terminal_impl(&app).map_err(|e| e.to_string())
}

pub fn open_terminal_impl(app: &AppHandle) -> anyhow::Result<TerminalSnapshot> {
    let state = app.state::<AppState>();
    let terminal_id = TerminalId::new();
    let pane_id = PaneId::new();
    let label = state.allocate_label();
    let created_at = Utc::now();

    let pty = spawn_pty(app, pane_id, None)?;

    let mut panes = HashMap::new();
    panes.insert(
        pane_id,
        PaneData {
            pty,
            foreground_command: None,
            exit_code: None,
        },
    );
    let terminal = Terminal {
        id: terminal_id,
        label,
        created_at,
        panes,
        layout: LayoutNode::Leaf(pane_id),
        active_pane: pane_id,
    };
    let snapshot = terminal.snapshot();
    state.insert(terminal);

    let _ = app.emit("terminal:added", snapshot.clone());
    Ok(snapshot)
}

#[tauri::command]
pub fn close_terminal(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    terminal_id: String,
) -> Result<(), String> {
    let id = parse_terminal_id(&terminal_id)?;
    let term = state
        .remove_terminal(id)
        .ok_or_else(|| format!("terminal {} not found", id))?;
    for (_, p) in term.panes {
        p.pty.kill();
    }
    let _ = app.emit("terminal:removed", terminal_id);
    Ok(())
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
    let new_pty = spawn_pty(&app, new_pane, None).map_err(|e| e.to_string())?;
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
            if let Some(term) = state.remove_terminal(terminal_id) {
                for (_, p) in term.panes {
                    p.pty.kill();
                }
                let _ = app.emit("terminal:removed", terminal_id.to_string());
            }
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
    if !state.rename_terminal(id, trimmed.to_string()) {
        return Err(format!("terminal {} not found", id));
    }
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
    let _ = app.emit("terminals:reordered", ids);
    Ok(())
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

/// Called once from the Tauri Builder `setup` hook.
/// Creates the first terminal (Term#1) and starts the foreground monitor.
pub fn bootstrap(app: AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    open_terminal_impl(&app)?;
    crate::monitor::start(app);
    Ok(())
}
