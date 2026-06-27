mod commands;
mod menu;
mod model;
mod monitor;
mod persist;
mod preferences;
mod pty;
mod shortcuts;
mod state;

use state::AppState;
use tauri::{Emitter, Manager, WindowEvent};

// WKWebView caches the bundled frontend per origin and keeps serving the stale
// bundle after an app update, so freshly embedded assets never take effect.
// On a version change, purge only the on-disk HTTP cache before the webview is
// created, so it refetches the fresh assets on this launch. localStorage (theme,
// sidebar width) lives in a separate store and is deliberately left intact, so
// user preferences survive updates.
fn flush_webview_cache_on_upgrade() {
    let current = env!("CARGO_PKG_VERSION");
    let mut state = persist::load();
    if state.app_version.as_str() == current {
        return;
    }
    purge_webview_http_cache();
    state.app_version = current.to_string();
    if let Err(e) = persist::save(&state) {
        tracing::warn!("failed to persist app_version: {e}");
    }
}

// The WebKit network/CacheStorage caches live under ~/Library/Caches/<id>,
// separate from localStorage in ~/Library/WebKit/<id>/WebsiteData. Deleting the
// former flushes the stale frontend without touching user preferences.
#[cfg(target_os = "macos")]
fn purge_webview_http_cache() {
    let Some(home) = dirs::home_dir() else { return };
    let dir = home.join("Library/Caches/com.termpane");
    if let Err(e) = std::fs::remove_dir_all(&dir) {
        if e.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!("failed to purge webview cache: {e}");
        }
    }
}

// Linux (WebKitGTK) uses a different cache layout; wire this up when Linux
// packaging is finalized.
#[cfg(not(target_os = "macos"))]
fn purge_webview_http_cache() {}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("termpane_lib=info,warn")),
        )
        .init();

    // Must run before the webview is created so the stale cache is gone by the
    // time it loads the frontend.
    flush_webview_cache_on_upgrade();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .menu(|handle| menu::build(handle))
        .on_menu_event(|_app, event| {
            if event.id().as_ref() == menu::NEW_WINDOW_ID {
                menu::spawn_new_instance();
            }
        })
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::list_terminals,
            commands::open_terminal,
            commands::take_startup_command,
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
            commands::get_shortcuts,
            commands::set_shortcuts,
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
