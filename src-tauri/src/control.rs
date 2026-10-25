//! Local control socket. Every TermPane instance (each window is its own
//! process) listens on `<run dir>/<pid>.sock`, user-only permissions. The same
//! binary in CLI mode talks to it: `termpane hook` forwards Claude Code hook
//! events, and `termpane mcp` lets any Claude session list, read, type into,
//! open, focus and close tabs across all running instances.
//!
//! Wire format: one JSON request line in, one JSON response line out.

use crate::model::{PaneId, TerminalId};
use crate::state::AppState;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

pub const ENV_SOCKET: &str = "TERMPANE_SOCKET";
pub const ENV_PANE: &str = "TERMPANE_PANE_ID";
/// This binary's path, so the Claude Code plugin can call `termpane hook`,
/// `termpane tool` and `termpane fork` from inside a TermPane shell.
pub const ENV_BIN: &str = "TERMPANE_BIN";

// sockaddr_un.sun_path is 104 bytes on macOS, 108 on Linux.
const MAX_SOCKET_PATH: usize = 100;

pub fn run_dir() -> PathBuf {
    let preferred = dirs::data_dir().map(|d| d.join("TermPane").join("run"));
    match preferred {
        Some(p) if p.join("4194304.sock").to_string_lossy().len() <= MAX_SOCKET_PATH => p,
        _ => std::env::temp_dir().join(format!("termpane-{}", nix::unistd::Uid::current())),
    }
}

pub fn own_socket_path() -> PathBuf {
    run_dir().join(format!("{}.sock", std::process::id()))
}

fn ensure_private_dir(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(dir)?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
}

/// Binds this instance's socket and serves it on background threads.
pub fn start(app: AppHandle) {
    let path = own_socket_path();
    if let Err(e) = path.parent().map(ensure_private_dir).unwrap_or(Ok(())) {
        tracing::warn!("control socket dir: {e}");
        return;
    }
    let _ = std::fs::remove_file(&path);
    let listener = match UnixListener::bind(&path) {
        Ok(l) => l,
        Err(e) => {
            tracing::warn!("control socket bind {}: {e}", path.display());
            return;
        }
    };
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    std::thread::Builder::new()
        .name("control-socket".into())
        .spawn(move || {
            for conn in listener.incoming() {
                let Ok(stream) = conn else { continue };
                let app = app.clone();
                std::thread::spawn(move || serve(app, stream));
            }
        })
        .expect("failed to start control socket thread");
}

pub fn remove_own_socket() {
    let _ = std::fs::remove_file(own_socket_path());
}

fn serve(app: AppHandle, stream: UnixStream) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let mut reader = BufReader::new(&stream);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return;
    }
    let response = match serde_json::from_str::<Value>(&line) {
        Ok(req) => handle(&app, &req).unwrap_or_else(|e| json!({ "ok": false, "error": e })),
        Err(e) => json!({ "ok": false, "error": format!("bad request: {e}") }),
    };
    let mut w = &stream;
    let _ = writeln!(w, "{response}");
}

fn handle(app: &AppHandle, req: &Value) -> Result<Value, String> {
    let state = app.state::<AppState>();
    match req["op"].as_str().unwrap_or_default() {
        "hook" => {
            let pane = parse_pane(&req["pane"])?;
            crate::claude::apply_hook(app, pane, &req["payload"]);
            Ok(json!({ "ok": true }))
        }
        "list" => Ok(json!({ "ok": true, "instance": std::process::id(), "tabs": list_tabs(&state) })),
        "read" => {
            let pane = resolve_pane(&state, req)?;
            let lines = req["lines"].as_u64().unwrap_or(100).clamp(1, READ_ROWS as u64) as usize;
            let pty = state.pty_of(pane).ok_or("pane not found")?;
            let (bytes, _) = pty.backlog.lock().snapshot();
            let cols = pty.cols().filter(|c| *c > 0).unwrap_or(120);
            Ok(json!({ "ok": true, "pane": pane.to_string(), "text": readable_tail(&bytes, cols, lines) }))
        }
        "send" => {
            let pane = resolve_pane(&state, req)?;
            let pty = state.pty_of(pane).ok_or("pane not found")?;
            let mut text = req["text"].as_str().unwrap_or_default().to_string();
            if req["enter"].as_bool().unwrap_or(true) {
                text.push('\r');
            }
            pty.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
            Ok(json!({ "ok": true, "pane": pane.to_string() }))
        }
        "open" => {
            let parent = match req["parent"].as_str() {
                Some(r) => Some(state.find_terminal(r).ok_or("parent tab not found")?),
                None => None,
            };
            let snap = crate::commands::open_terminal_impl(
                app,
                req["cwd"].as_str(),
                req["command"].as_str(),
                None,
                req["name"].as_str(),
                parent,
                None,
            )
            .map_err(|e| e.to_string())?;
            Ok(json!({ "ok": true, "tab": snap.id, "pane": snap.active_pane, "label": snap.label }))
        }
        "fork" => {
            let pane = parse_pane(&req["pane"])?;
            let (tab, label) = fork_session(app, pane, req)?;
            Ok(json!({ "ok": true, "tab": tab, "label": label }))
        }
        "close" => {
            let tab = resolve_tab(&state, req)?;
            crate::commands::close_terminal_impl(app, tab)?;
            Ok(json!({ "ok": true }))
        }
        "focus" => {
            let tab = resolve_tab(&state, req)?;
            state.set_active(tab);
            let _ = app.emit("terminal:focus-requested", tab.to_string());
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
            Ok(json!({ "ok": true }))
        }
        other => Err(format!("unknown op: {other}")),
    }
}

/// Opens a sub-tab under the pane's tab running a fork of its Claude session
/// (`claude --resume <id> --fork-session`), named "FORK - <tab>", in Claude's
/// working directory, and brings it to the front.
pub fn fork_session(app: &AppHandle, pane: PaneId, req: &Value) -> Result<(String, String), String> {
    let state = app.state::<AppState>();
    let session = req["session"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| state.claude_of(pane).map(|c| c.session_id))
        .ok_or("no Claude session known for this pane")?;
    let parent = state
        .snapshots()
        .into_iter()
        .find(|t| t.panes.iter().any(|p| p.id == pane.to_string()))
        .ok_or("pane not found")?;
    let name = req["name"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| {
            if parent.label.starts_with("FORK - ") {
                parent.label.clone()
            } else {
                format!("FORK - {}", parent.label)
            }
        });
    let mut command = format!(
        "claude --resume {} --fork-session -n {}",
        shell_quote(&session),
        shell_quote(&name)
    );
    if let Some(prompt) = req["prompt"].as_str().map(str::trim).filter(|p| !p.is_empty()) {
        command.push(' ');
        command.push_str(&shell_quote(prompt));
    }
    let parent_id: TerminalId = parent.id.parse().map_err(|_| "bad terminal id")?;
    let cwd = crate::commands::pane_cwd(&state, pane);
    let snap = crate::commands::open_terminal_impl(
        app,
        cwd.as_deref(),
        Some(&command),
        None,
        Some(&name),
        Some(parent_id),
        Some(false),
    )
    .map_err(|e| e.to_string())?;
    if let Ok(id) = snap.id.parse::<TerminalId>() {
        state.set_active(id);
    }
    let _ = app.emit("terminal:focus-requested", snap.id.clone());
    Ok((snap.id, snap.label))
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn parse_pane(v: &Value) -> Result<PaneId, String> {
    v.as_str()
        .ok_or("missing pane")?
        .parse::<PaneId>()
        .map_err(|e| format!("invalid pane id: {e}"))
}

fn resolve_tab(state: &AppState, req: &Value) -> Result<TerminalId, String> {
    let r = req["tab"].as_str().ok_or("missing tab")?;
    state.find_terminal(r).ok_or_else(|| format!("tab not found: {r}"))
}

fn resolve_pane(state: &AppState, req: &Value) -> Result<PaneId, String> {
    if req["pane"].is_string() {
        let pane = parse_pane(&req["pane"])?;
        return state.pty_of(pane).map(|_| pane).ok_or_else(|| "pane not found".into());
    }
    let tab = resolve_tab(state, req)?;
    state.active_pane_of(tab).ok_or_else(|| "tab has no panes".into())
}

fn list_tabs(state: &AppState) -> Vec<Value> {
    state
        .snapshots()
        .into_iter()
        .map(|t| {
            let panes: Vec<Value> = t
                .panes
                .iter()
                .map(|p| {
                    let cwd = p
                        .id
                        .parse::<PaneId>()
                        .ok()
                        .and_then(|id| crate::commands::pane_cwd(state, id));
                    json!({
                        "id": p.id,
                        "cwd": cwd,
                        "running": p.foreground_command,
                        "claude": p.claude.as_ref().map(|c| json!({
                            "status": c.status,
                            "detail": c.detail,
                            "sessionId": c.session_id,
                            "filesChanged": c.files,
                            "background": c.background.iter().map(|b| json!({
                                "id": b.id, "kind": b.kind, "description": b.description,
                            })).collect::<Vec<_>>(),
                            "artifacts": c.artifacts,
                        })),
                        "git": p.git,
                        "ports": p.ports,
                    })
                })
                .collect();
            json!({
                "id": t.id,
                "label": t.label,
                "parentId": t.parent_id,
                "activePane": t.active_pane,
                "markedUnread": t.marked_unread,
                "panes": panes,
            })
        })
        .collect()
}

// Height of the virtual screen used to render a pane's backlog for reading.
const READ_ROWS: u16 = 1000;

/// The last `lines` lines of a pane's output as plain text. The raw backlog is
/// replayed through a terminal emulator at the pane's width, so cursor moves,
/// line editing, progress bars and redraws come out as they looked on screen.
pub(crate) fn readable_tail(bytes: &[u8], cols: u16, lines: usize) -> String {
    let mut parser = vt100::Parser::new(READ_ROWS, cols, 0);
    parser.process(bytes);
    let text = parser.screen().contents();
    let mut out: Vec<&str> = text.lines().map(str::trim_end).collect();
    while out.last().is_some_and(|l| l.is_empty()) {
        out.pop();
    }
    let start = out.len().saturating_sub(lines);
    out[start..].join("\n")
}

// --- Client side (CLI mode) --------------------------------------------------

pub fn request(socket: &Path, req: &Value, timeout: Duration) -> Result<Value, String> {
    let stream = UnixStream::connect(socket).map_err(|e| e.to_string())?;
    stream.set_read_timeout(Some(timeout)).map_err(|e| e.to_string())?;
    stream.set_write_timeout(Some(timeout)).map_err(|e| e.to_string())?;
    let mut w = &stream;
    writeln!(w, "{req}").map_err(|e| e.to_string())?;
    let mut line = String::new();
    BufReader::new(&stream).read_line(&mut line).map_err(|e| e.to_string())?;
    let v: Value = serde_json::from_str(&line).map_err(|e| format!("bad response: {e}"))?;
    if v["ok"] == true {
        Ok(v)
    } else {
        Err(v["error"].as_str().unwrap_or("request failed").to_string())
    }
}

/// Sockets of every running instance. Leftovers from instances that died
/// without cleaning up are removed on the way.
pub fn instances() -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(run_dir()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in entries.flatten() {
        let path = e.path();
        if path.extension().is_none_or(|x| x != "sock") {
            continue;
        }
        match UnixStream::connect(&path) {
            Ok(_) => out.push(path),
            Err(err) if err.kind() == std::io::ErrorKind::ConnectionRefused => {
                let _ = std::fs::remove_file(&path);
            }
            Err(_) => {}
        }
    }
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::readable_tail;

    #[test]
    fn renders_line_editing_and_cursor_moves() {
        // zsh echoing a typed command and redrawing it, `ls` aligning columns
        // with cursor-forward, and a progress bar overwritten with \r.
        let raw = b"$ e\x08echo hi\r\nhi\r\na.txt\x1b[3Cb.txt\r\n10%\r50%\r100%\r\n";
        assert_eq!(
            readable_tail(raw, 40, 10),
            "$ echo hi\nhi\na.txt   b.txt\n100%"
        );
        assert_eq!(readable_tail(raw, 40, 1), "100%");
    }
}
