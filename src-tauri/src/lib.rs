mod commands;
mod model;
mod monitor;
mod persist;
mod preferences;
mod pty;
mod state;

use state::AppState;

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
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            commands::bootstrap(handle)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to run TermPane application");
}
