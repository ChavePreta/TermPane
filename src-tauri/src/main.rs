// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // CLI modes used by Claude Code (hooks and the MCP server); see cli.rs.
    match std::env::args().nth(1).as_deref() {
        Some("hook") => termpane_lib::cli::run_hook(),
        Some("mcp") => termpane_lib::cli::run_mcp(),
        Some("tool") => termpane_lib::cli::run_tool(),
        Some("fork") => termpane_lib::cli::run_fork(),
        Some("watch") => termpane_lib::cli::run_watch(),
        _ => termpane_lib::run(),
    }
}
