//! CLI modes of the TermPane binary, dispatched from `main` before any GUI
//! setup. Neither prints anything except protocol output.
//!
//! - `termpane hook`: Claude Code hook command. Reads the hook payload on stdin
//!   and forwards it to the TermPane instance that owns this shell. Always exits
//!   0 silently so it can never block or alter Claude.
//! - `termpane watch`: what TermPane asks of the Claude session in this shell,
//!   streamed to the plugin.
//! - `termpane mcp`: stdio MCP server exposing tab control across every running
//!   TermPane instance.

use crate::control::{self, ENV_PANE, ENV_SOCKET};
use serde_json::{json, Map, Value};
use std::io::{BufRead, Read, Write};
use std::path::PathBuf;
use std::time::Duration;

/// `termpane tool --list` prints the tool specs as JSON; `termpane tool <name>
/// [<json args>]` runs one and prints its text. Used by the Claude Code plugin,
/// which registers the same tools the MCP server serves.
pub fn run_tool() {
    let mut args = std::env::args().skip(2);
    let name = args.next().unwrap_or_default();
    if name == "--list" {
        let specs: Vec<Value> = tools()
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|mut t| {
                let schema = t["inputSchema"].take();
                json!({ "name": t["name"], "description": t["description"], "inputSchema": schema })
            })
            .collect();
        println!("{}", Value::Array(specs));
        return;
    }
    let input = args
        .next()
        .and_then(|a| serde_json::from_str::<Value>(&a).ok())
        .unwrap_or_else(|| json!({}));
    match call_tool(&name, &input) {
        Ok(text) => println!("{text}"),
        Err(e) => {
            println!("{e}");
            std::process::exit(1);
        }
    }
}

/// `termpane fork --session <id> [--prompt <text>] [--name <label>]`: run by the
/// plugin's /fork inside a TermPane shell. Opens the fork in a sub-tab of this
/// shell's tab; exits non-zero (so the built-in /fork runs instead) on failure.
pub fn run_fork() {
    let (Ok(socket), Ok(pane)) = (std::env::var(ENV_SOCKET), std::env::var(ENV_PANE)) else {
        eprintln!("not inside a TermPane shell");
        std::process::exit(1);
    };
    let mut req = json!({ "op": "fork", "pane": pane });
    let mut args = std::env::args().skip(2);
    while let Some(flag) = args.next() {
        let value = args.next().unwrap_or_default();
        match flag.as_str() {
            "--session" => req["session"] = json!(value),
            "--prompt" => req["prompt"] = json!(value),
            "--name" => req["name"] = json!(value),
            _ => {}
        }
    }
    match control::request(&PathBuf::from(socket), &req, TIMEOUT) {
        Ok(v) => println!(
            "Forked into TermPane tab \"{}\", nested under this one.",
            v["label"].as_str().unwrap_or_default()
        ),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}

/// `termpane watch`: run by the plugin for the life of a Claude session in a
/// TermPane shell. Prints one JSON line per thing TermPane asks of the session
/// (`{"rename": "<name>"}` when the user renames the tab) until TermPane
/// closes the connection.
pub fn run_watch() {
    let (Ok(socket), Ok(pane)) = (std::env::var(ENV_SOCKET), std::env::var(ENV_PANE)) else {
        std::process::exit(1);
    };
    let Ok(stream) = std::os::unix::net::UnixStream::connect(&socket) else {
        std::process::exit(1);
    };
    let mut w = &stream;
    if writeln!(w, "{}", json!({ "op": "watch", "pane": pane })).is_err() {
        std::process::exit(1);
    }
    let mut lines = std::io::BufReader::new(&stream).lines();
    match lines.next() {
        Some(Ok(first)) if serde_json::from_str::<Value>(&first).is_ok_and(|v| v["ok"] == true) => {}
        _ => std::process::exit(1),
    }
    let mut out = std::io::stdout();
    for line in lines.map_while(Result::ok) {
        if writeln!(out, "{line}").and_then(|_| out.flush()).is_err() {
            break;
        }
    }
}

pub fn run_hook() {
    let (Ok(socket), Ok(pane)) = (std::env::var(ENV_SOCKET), std::env::var(ENV_PANE)) else {
        return;
    };
    let mut raw = String::new();
    if std::io::stdin().take(8 * 1024 * 1024).read_to_string(&mut raw).is_err() {
        return;
    }
    let Ok(payload) = serde_json::from_str::<Value>(&raw) else { return };
    let req = json!({ "op": "hook", "pane": pane, "payload": slim_payload(&payload) });
    let _ = control::request(&PathBuf::from(socket), &req, Duration::from_secs(1));
}

/// Keeps only what TermPane reads; tool inputs and responses can be megabytes.
fn slim_payload(p: &Value) -> Value {
    let mut out = Map::new();
    for key in [
        "hook_event_name",
        "session_id",
        "cwd",
        "tool_name",
        "message",
        "notification_type",
        "task",
        "artifact",
        "forced",
        "title",
    ] {
        if let Some(v) = p.get(key) {
            out.insert(key.to_string(), v.clone());
        }
    }
    let mut input = Map::new();
    for key in ["file_path", "notebook_path", "pattern", "description", "url", "query"] {
        if let Some(v) = p["tool_input"].get(key) {
            input.insert(key.to_string(), v.clone());
        }
    }
    if let Some(cmd) = p["tool_input"]["command"].as_str() {
        input.insert("command".into(), json!(cmd.chars().take(200).collect::<String>()));
    }
    out.insert("tool_input".into(), Value::Object(input));
    Value::Object(out)
}

// --- MCP ---------------------------------------------------------------------

const TAB_REF: &str = "Tab id or exact tab label (case-insensitive), as returned by list_tabs.";

fn tools() -> Value {
    json!([
        {
            "name": "list_tabs",
            "description": "List every TermPane tab across all open TermPane windows on this machine: id, label, parent tab (sub-tabs), and for each pane its working directory, the command running in it, and — when Claude Code runs there — its status (thinking, tool, permission, waiting, done) and the files that session changed. Call this first to find a tab.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "read_output",
            "description": "Read the most recent output of a TermPane tab (its active pane) or a specific pane, as plain text with colors stripped. Use it to check a dev server, test run or build in another tab.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "tab": { "type": "string", "description": TAB_REF },
                    "pane": { "type": "string", "description": "Pane id (overrides tab)." },
                    "lines": { "type": "integer", "description": "How many trailing lines to return (default 100, max 2000)." }
                }
            }
        },
        {
            "name": "send_input",
            "description": "Type text into a TermPane tab (its active pane) or a specific pane, as if the user typed it. By default presses Enter afterwards, so the text runs as a command.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "tab": { "type": "string", "description": TAB_REF },
                    "pane": { "type": "string", "description": "Pane id (overrides tab)." },
                    "text": { "type": "string", "description": "Text to type." },
                    "press_enter": { "type": "boolean", "description": "Press Enter after the text (default true)." }
                },
                "required": ["text"]
            }
        },
        {
            "name": "open_tab",
            "description": "Open a new TermPane tab running an interactive shell, optionally in a directory, with a startup command and a name. With parent_tab it opens as a sub-tab nested under that tab. Opens in the TermPane window you are running in, or the one that owns parent_tab.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "cwd": { "type": "string", "description": "Working directory (absolute or ~/...)." },
                    "command": { "type": "string", "description": "Command typed into the shell after it starts; the tab stays open when it exits." },
                    "name": { "type": "string", "description": "Tab label." },
                    "parent_tab": { "type": "string", "description": TAB_REF }
                }
            }
        },
        {
            "name": "focus_tab",
            "description": "Bring a TermPane tab to the front so the user sees it.",
            "inputSchema": {
                "type": "object",
                "properties": { "tab": { "type": "string", "description": TAB_REF } },
                "required": ["tab"]
            }
        },
        {
            "name": "close_tab",
            "description": "Close a TermPane tab, killing every process running in it. Its sub-tabs stay open and move up a level.",
            "inputSchema": {
                "type": "object",
                "properties": { "tab": { "type": "string", "description": TAB_REF } },
                "required": ["tab"]
            }
        }
    ])
}

pub fn run_mcp() {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
        let Some(id) = msg.get("id").cloned() else { continue };
        let reply = match msg["method"].as_str().unwrap_or_default() {
            "initialize" => Ok(json!({
                "protocolVersion": msg["params"]["protocolVersion"].as_str().unwrap_or("2025-06-18"),
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "termpane", "version": env!("CARGO_PKG_VERSION") },
                "instructions": "Controls tabs of the TermPane terminal manager. Use list_tabs to discover tabs, read_output to watch another tab (dev server, tests), send_input to run commands there, and open_tab with parent_tab to start helper tabs nested under the user's tab."
            })),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tools() })),
            "tools/call" => {
                let name = msg["params"]["name"].as_str().unwrap_or_default();
                let args = &msg["params"]["arguments"];
                Ok(match call_tool(name, args) {
                    Ok(text) => json!({ "content": [{ "type": "text", "text": text }] }),
                    Err(e) => json!({ "content": [{ "type": "text", "text": e }], "isError": true }),
                })
            }
            other => Err(json!({ "code": -32601, "message": format!("method not found: {other}") })),
        };
        let out = match reply {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err(error) => json!({ "jsonrpc": "2.0", "id": id, "error": error }),
        };
        if writeln!(stdout, "{out}").is_err() || stdout.flush().is_err() {
            break;
        }
    }
}

const TIMEOUT: Duration = Duration::from_secs(10);

fn call_tool(name: &str, args: &Value) -> Result<String, String> {
    let instances = control::instances();
    if instances.is_empty() {
        return Err("TermPane is not running.".into());
    }
    let s = |k: &str| args[k].as_str().map(str::to_string);
    match name {
        "list_tabs" => {
            let mut windows = Vec::new();
            for sock in &instances {
                if let Ok(v) = control::request(sock, &json!({ "op": "list" }), TIMEOUT) {
                    windows.push(json!({ "window": v["instance"], "tabs": v["tabs"] }));
                }
            }
            Ok(serde_json::to_string_pretty(&windows).unwrap_or_default())
        }
        "read_output" => {
            let v = first_success(&instances, &json!({
                "op": "read", "tab": s("tab"), "pane": s("pane"), "lines": args["lines"],
            }))?;
            Ok(v["text"].as_str().unwrap_or_default().to_string())
        }
        "send_input" => {
            first_success(&instances, &json!({
                "op": "send", "tab": s("tab"), "pane": s("pane"),
                "text": s("text").unwrap_or_default(),
                "enter": args["press_enter"].as_bool().unwrap_or(true),
            }))?;
            Ok("Sent.".into())
        }
        "open_tab" => {
            let req = json!({
                "op": "open", "cwd": s("cwd"), "command": s("command"), "name": s("name"),
                "parent": s("parent_tab"),
            });
            let v = if s("parent_tab").is_some() {
                first_success(&instances, &req)?
            } else {
                let own = std::env::var(ENV_SOCKET).ok().map(PathBuf::from);
                let target = own.filter(|p| instances.contains(p)).unwrap_or_else(|| instances[0].clone());
                control::request(&target, &req, TIMEOUT)?
            };
            Ok(format!(
                "Opened tab \"{}\" (id {}, pane {}).",
                v["label"].as_str().unwrap_or_default(),
                v["tab"].as_str().unwrap_or_default(),
                v["pane"].as_str().unwrap_or_default()
            ))
        }
        "focus_tab" => {
            first_success(&instances, &json!({ "op": "focus", "tab": s("tab") }))?;
            Ok("Focused.".into())
        }
        "close_tab" => {
            first_success(&instances, &json!({ "op": "close", "tab": s("tab") }))?;
            Ok("Closed.".into())
        }
        other => Err(format!("unknown tool: {other}")),
    }
}

/// Tabs and panes are unique across instances, so ask each until one owns it.
fn first_success(instances: &[PathBuf], req: &Value) -> Result<Value, String> {
    let mut last = String::from("not found");
    for sock in instances {
        match control::request(sock, req, TIMEOUT) {
            Ok(v) => return Ok(v),
            Err(e) => last = e,
        }
    }
    Err(last)
}
