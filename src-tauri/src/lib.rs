mod commands;
mod menu;
mod model;
mod monitor;
mod persist;
mod preferences;
mod pty;
mod state;

use state::AppState;
use tauri::{Emitter, Manager, WindowEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("termpane_lib=info,warn")),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .menu(|handle| menu::build(handle))
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::list_terminals,
            commands::open_terminal,
            commands::close_terminal,
            commands::split_pane,
            commands::close_pane,
            commands::focus_pane,
            commands::write_input,
            commands::resize_pane,
            commands::set_layout_ratios,
            commands::active_terminal,
            commands::set_active_terminal,
            commands::rename_terminal,
            commands::reorder_terminals,
            commands::set_always_on_top,
            commands::get_preferences,
            commands::set_preferences,
            commands::extract_pane,
            commands::merge_terminal_into_pane,
            commands::flip_parent_split,
            commands::write_input_broadcast,
            commands::get_platform,
            commands::quit_app,
        ])
        .on_window_event(|window, event| {
            // Intercept the window's red-X close: if any pane has a foreground
            // process running, prevent the close and ask the frontend to show
            // a confirmation dialog. The frontend calls `quit_app` to actually
            // destroy the window once the user confirms.
            if let WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                let state = app.state::<AppState>();
                let running = state.running_foreground_commands();
                if !running.is_empty() {
                    api.prevent_close();
                    let _ = app.emit("app:close-requested", running);
                }
            }
        })
        .setup(|app| {
            let handle = app.handle().clone();
            commands::bootstrap(handle)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to run TermPane application");
}
